import * as duckdb from 'https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.28.1-dev106.0/+esm';

const MONTHLY_LIMIT = 1000;
let map;
let geocoder;
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

function getGeocodeCount() {
    const month = new Date().toISOString().slice(0, 7); // YYYY-MM
    const key = `geocode_count_${month}`;
    return parseInt(localStorage.getItem(key) || '0', 10);
}

function incrementGeocodeCount() {
    const month = new Date().toISOString().slice(0, 7);
    const key = `geocode_count_${month}`;
    const current = getGeocodeCount();
    localStorage.setItem(key, (current + 1).toString());
    return current + 1;
}

function updateGeocodeStats() {
    document.getElementById('stat-geocoded').textContent = `${getGeocodeCount()} / ${MONTHLY_LIMIT}`;
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
        geocoder = new google.maps.Geocoder();
        updateGeocodeStats();

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

        // 3. Query Lots
        const query = await conn.query(`
            SELECT 
                c.lot_lelang_id, 
                c.nama_lot_lelang, 
                c.nilai_limit, 
                c.status,
                COALESCE(d.barangs_json->0->>'alamat', c.nama_lokasi) as address
            FROM 'catalog.parquet' c
            LEFT JOIN 'details.parquet' d ON c.lot_lelang_id = d.lot_lelang_id
            WHERE address IS NOT NULL AND address != ''
            LIMIT 500
        `);
        const rows = query.toArray();
        
        document.getElementById('stat-loaded').textContent = rows.length.toString();
        setStatus(`Found ${rows.length} lots. Geocoding...`, "loading");
        
        // 4. Geocode and Plot
        const progressContainer = document.getElementById('geocoding-progress-container');
        const progressFill = document.getElementById('geocoding-progress-fill');
        progressContainer.style.display = 'block';

        let processed = 0;

        for (const row of rows) {
            processed++;
            progressFill.style.width = `${(processed / rows.length) * 100}%`;
            
            if (!row.address) continue;
            
            const cacheKey = `geo_${row.address}`;
            let location = null;
            
            // Check cache
            const cachedStr = localStorage.getItem(cacheKey);
            if (cachedStr) {
                try {
                    location = JSON.parse(cachedStr);
                } catch(e) {}
            }

            if (!location) {
                // Check quota
                if (getGeocodeCount() >= MONTHLY_LIMIT) {
                    setStatus("Geocoding limit reached for this month.", "error");
                    break;
                }

                // API Call
                try {
                    const result = await new Promise((resolve, reject) => {
                        geocoder.geocode({ address: row.address }, (results, status) => {
                            if (status === 'OK') {
                                resolve({
                                    lat: results[0].geometry.location.lat(),
                                    lng: results[0].geometry.location.lng()
                                });
                            } else {
                                reject(status);
                            }
                        });
                    });
                    
                    location = result;
                    localStorage.setItem(cacheKey, JSON.stringify(location));
                    incrementGeocodeCount();
                    updateGeocodeStats();
                    
                    // Throttle
                    await new Promise(r => setTimeout(r, 400));
                } catch (e) {
                    console.warn("Geocode failed for:", row.address, e);
                    // Add delay even on failure to avoid hitting limits harder
                    await new Promise(r => setTimeout(r, 500));
                    continue;
                }
            }

            // Create Marker
            if (location) {
                const marker = new google.maps.Marker({
                    position: location,
                    map: map,
                    title: row.nama_lot_lelang
                });
                
                const infoWindow = new google.maps.InfoWindow({
                    content: `
                        <div class="info-window">
                            <span class="status">${row.status || 'UNKNOWN'}</span>
                            <h3>${row.nama_lot_lelang}</h3>
                            <p>${row.address}</p>
                            <p class="price">Rp ${Number(row.nilai_limit).toLocaleString()}</p>
                        </div>
                    `
                });

                marker.addListener("click", () => {
                    infoWindow.open({
                        anchor: marker,
                        map,
                    });
                });
                
                markers.push(marker);
            }
        }
        
        setStatus("Geocoding Complete", "success");
        setTimeout(() => {
            progressContainer.style.display = 'none';
        }, 2000);

    } catch (e) {
        console.error(e);
        setStatus("Error: " + e.message, "error");
    }
}

init();
