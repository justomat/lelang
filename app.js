import * as duckdb from 'https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.28.1-dev106.0/+esm';

let map;
let activeInfoWindow = null;
const markers = [];

async function getApiKey() {
    try {
        const res = await fetch('.env');
        if (res.ok) {
            const text = await res.text();
            const match = text.match(/GOOGLE_MAPS_API_KEY=(.+)/);
            if (match && match[1]) {
                return match[1].trim();
            }
        }
    } catch (e) {
        console.warn("Could not fetch .env file, checking localStorage");
    }

    const cachedKey = localStorage.getItem('gmaps_api_key');
    if (cachedKey) return cachedKey;

    return new Promise((resolve) => {
        const modal = document.getElementById('api-key-modal');
        const input = document.getElementById('api-key-input');
        const btn = document.getElementById('save-api-key-btn');
        
        modal.style.display = 'flex';
        
        btn.onclick = () => {
            const val = input.value.trim();
            if (val) {
                localStorage.setItem('gmaps_api_key', val);
                modal.style.display = 'none';
                resolve(val);
            }
        };
    });
}

function loadGoogleMaps(apiKey) {
    return new Promise((resolve, reject) => {
        if (window.google && window.google.maps) {
            resolve();
            return;
        }
        window.initMap = () => {
            resolve();
        };
        const script = document.createElement('script');
        script.src = `https://maps.googleapis.com/maps/api/js?key=${apiKey}&callback=initMap`;
        script.async = true;
        script.defer = true;
        script.onerror = reject;
        document.head.appendChild(script);
    });
}

async function init() {
    const statusText = document.getElementById('status-text');
    const statusIndicator = document.getElementById('status-indicator');

    function setStatus(msg, type) {
        statusText.textContent = msg;
        statusIndicator.className = `pulse ${type}`;
    }

    try {
        // 1. Setup API & Map
        setStatus("Waiting for API Key...", "loading");
        const apiKey = await getApiKey();
        
        setStatus("Loading Google Maps...", "loading");
        await loadGoogleMaps(apiKey);
        
        map = new google.maps.Map(document.getElementById("map"), {
            center: { lat: -2.5489, lng: 118.0149 }, // Center of Indonesia
            zoom: 5,
            mapId: "DEMO_MAP_ID",
            disableDefaultUI: true,
            zoomControl: true,
        });

        // 2. Load DuckDB & Data
        setStatus("Initializing DuckDB Engine...", "loading");
        
        const fetchCatPromise = fetch('data/catalog_lots.parquet').then(res => {
            if (!res.ok) throw new Error("Failed to fetch catalog_lots.parquet");
            return res.arrayBuffer();
        });
        const fetchDetPromise = fetch('data/lot_details.parquet').then(res => {
            if (!res.ok) throw new Error("Failed to fetch lot_details.parquet");
            return res.arrayBuffer();
        });

        const JSDELIVR_BUNDLES = duckdb.getJsDelivrBundles();
        const bundle = await duckdb.selectBundle(JSDELIVR_BUNDLES);
        const worker_url = URL.createObjectURL(
            new Blob([`importScripts("${bundle.mainWorker}");`], { type: 'text/javascript' })
        );
        const worker = new Worker(worker_url);
        const logger = new duckdb.ConsoleLogger();
        const db = new duckdb.AsyncDuckDB(logger, worker);
        await db.instantiate(bundle.mainModule, bundle.pthreadWorker);
        URL.revokeObjectURL(worker_url);

        const [bufCat, bufDet] = await Promise.all([fetchCatPromise, fetchDetPromise]);

        await db.registerFileBuffer('catalog.parquet', new Uint8Array(bufCat));
        await db.registerFileBuffer('details.parquet', new Uint8Array(bufDet));

        const conn = await db.connect();
        
        setStatus("Extracting Lot Locations...", "loading");

        // 3. Query Lots with pre-computed coordinates
        const query = await conn.query(`
            SELECT 
                c.lot_lelang_id, 
                c.nama_lot_lelang, 
                c.nilai_limit, 
                c.status,
                COALESCE(d.barangs_json->0->>'alamat', c.nama_lokasi) as address,
                d.latitude as lat,
                d.longitude as lng
            FROM 'catalog.parquet' c
            LEFT JOIN 'details.parquet' d ON c.lot_lelang_id = d.lot_lelang_id
            WHERE d.latitude IS NOT NULL AND d.longitude IS NOT NULL
            LIMIT 500
        `);
        const rows = query.toArray();
        
        document.getElementById('stat-loaded').textContent = rows.length.toString();
        
        // 4. Plot Markers
        setStatus(`Plotting ${rows.length} lots...`, "loading");

        for (const row of rows) {
            const marker = new google.maps.Marker({
                position: { lat: row.lat, lng: row.lng },
                map: map,
                title: row.nama_lot_lelang
            });
            
            const infoWindow = new google.maps.InfoWindow({
                content: `
                    <div class="info-window">
                        <span class="status">${row.status || 'UNKNOWN'}</span>
                        <h3><a href="https://lelang.go.id/lot-lelang/detail/${row.lot_lelang_id}" target="_blank" style="text-decoration: none; color: inherit;">${row.nama_lot_lelang}</a></h3>
                        <p>${row.address || 'No Address'}</p>
                        <p class="price">Rp ${Number(row.nilai_limit).toLocaleString()}</p>
                    </div>
                `
            });

            marker.addListener("click", () => {
                if (activeInfoWindow) {
                    activeInfoWindow.close();
                }
                infoWindow.open({
                    anchor: marker,
                    map,
                });
                activeInfoWindow = infoWindow;
            });
            
            markers.push(marker);
        }
        
        setStatus("Map Ready", "success");

    } catch (e) {
        console.error(e);
        setStatus("Error: " + e.message, "error");
    }
}

init();