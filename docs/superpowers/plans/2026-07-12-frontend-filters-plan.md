# Frontend Map Filters & InfoWindow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a DuckDB WASM-backed filtering sidebar and fix Google Maps InfoWindow behavior (single open window, clickable title).

**Architecture:** Use DuckDB WASM for dynamic query generation upon filter application, clearing the map and rendering new markers matching the SQL conditions. Initialize dropdowns dynamically using distinct queries against the Parquet data.

**Tech Stack:** JavaScript, HTML, CSS, DuckDB WASM, Google Maps JS API.

---

### Task 1: InfoWindow Fixes & Single Active Window

**Files:**
- Modify: `app.js`

- [ ] **Step 1: Implement single active InfoWindow and update template**
Update `app.js` to define `activeInfoWindow`. Modify the click listener to close it before opening a new one, and change the template to wrap only the title in an `<a>` tag linking to the real site.

```javascript
// Add at the top of app.js below let map;
let activeInfoWindow = null;

// Replace the infoWindow creation and marker listener in the init() function (around line 112)
// Old code:
// const infoWindow = new google.maps.InfoWindow({ ... });
// marker.addListener("click", () => { ... });

// New code:
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
```

- [ ] **Step 2: Commit**

```bash
git add app.js
git commit -m "fix(ui): single active infowindow and clickable titles"
```

### Task 2: Create Sidebar Filter UI

**Files:**
- Modify: `index.html`
- Modify: `style.css`

- [ ] **Step 1: Add sidebar HTML structure to index.html**
Insert the new `<aside>` immediately before `<div id="status-panel">`.

```html
    <aside id="filter-sidebar" class="glass-panel">
        <h2>Filters</h2>
        <div class="filter-group">
            <h3>Kategori</h3>
            <label><input type="checkbox" id="cat-tanah" value="tanah"> Tanah</label>
            <label><input type="checkbox" id="cat-rumah" value="rumah"> Rumah</label>
            <label><input type="checkbox" id="cat-ruko" value="ruko"> Ruko</label>
        </div>
        
        <div class="filter-group">
            <h3>Harga Limit</h3>
            <input type="number" id="price-min" placeholder="Min Harga (Rp)">
            <input type="number" id="price-max" placeholder="Max Harga (Rp)">
        </div>
        
        <div class="filter-group">
            <h3>Lokasi</h3>
            <select id="sel-provinsi">
                <option value="">Semua Provinsi</option>
            </select>
            <select id="sel-kota">
                <option value="">Semua Kota/Kabupaten</option>
            </select>
        </div>
        
        <div class="filter-actions">
            <button id="btn-apply-filters">Terapkan</button>
            <button id="btn-reset-filters" class="secondary">Reset Filter</button>
        </div>
    </aside>
```

- [ ] **Step 2: Add CSS styles for the sidebar in style.css**
Append these styles to the bottom of `style.css`.

```css
#filter-sidebar {
    position: absolute;
    left: 20px;
    top: 20px;
    width: 280px;
    display: flex;
    flex-direction: column;
    gap: 20px;
    z-index: 10;
}

#filter-sidebar h2 {
    font-size: 1.2rem;
    margin: 0;
    color: var(--text-primary);
}

.filter-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
}

.filter-group h3 {
    font-size: 0.9rem;
    margin: 0;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
}

.filter-group label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 0.9rem;
    color: var(--text-primary);
    cursor: pointer;
}

.filter-group input[type="number"],
.filter-group select {
    width: 100%;
    padding: 8px 12px;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(0, 0, 0, 0.2);
    color: var(--text-primary);
    font-family: inherit;
    font-size: 0.9rem;
}

.filter-actions {
    display: flex;
    gap: 10px;
    margin-top: 10px;
}

.filter-actions button {
    flex: 1;
    padding: 10px;
    border: none;
    border-radius: 6px;
    background: var(--accent-color);
    color: white;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
}

.filter-actions button:hover {
    background: #2563eb;
}

.filter-actions button.secondary {
    background: rgba(255, 255, 255, 0.1);
}

.filter-actions button.secondary:hover {
    background: rgba(255, 255, 255, 0.2);
}
```

- [ ] **Step 3: Commit**

```bash
git add index.html style.css
git commit -m "feat(ui): add filter sidebar components"
```

### Task 3: Populate Dynamic Location Dropdowns

**Files:**
- Modify: `app.js`

- [ ] **Step 1: Write function to fetch and populate locations**
Create an async function `populateLocationDropdowns(conn)` and call it after the DB is initialized in `init()`.

```javascript
// Add above init() function
async function populateLocationDropdowns(conn) {
    try {
        const provQuery = await conn.query(`
            SELECT DISTINCT seller_provinsi 
            FROM 'details.parquet' 
            WHERE seller_provinsi IS NOT NULL AND seller_provinsi != '' 
            ORDER BY seller_provinsi
        `);
        const provinces = provQuery.toArray();
        const selProvinsi = document.getElementById('sel-provinsi');
        provinces.forEach(row => {
            const opt = document.createElement('option');
            opt.value = row.seller_provinsi;
            opt.textContent = row.seller_provinsi;
            selProvinsi.appendChild(opt);
        });

        const kotaQuery = await conn.query(`
            SELECT DISTINCT seller_kota 
            FROM 'details.parquet' 
            WHERE seller_kota IS NOT NULL AND seller_kota != '' 
            ORDER BY seller_kota
        `);
        const kotas = kotaQuery.toArray();
        const selKota = document.getElementById('sel-kota');
        kotas.forEach(row => {
            const opt = document.createElement('option');
            opt.value = row.seller_kota;
            opt.textContent = row.seller_kota;
            selKota.appendChild(opt);
        });
    } catch (e) {
        console.error("Failed to populate dropdowns", e);
    }
}

// Inside init(), right before "Extracting Lot Locations..." setStatus call:
        await populateLocationDropdowns(conn);
        // Save the connection for the filter button later
        window.dbConn = conn;
```

- [ ] **Step 2: Commit**

```bash
git add app.js
git commit -m "feat(db): dynamically populate location dropdowns from duckdb"
```

### Task 4: Implement Dynamic Filtering Logic

**Files:**
- Modify: `app.js`

- [ ] **Step 1: Refactor marker rendering into a reusable function**
Move the marker plotting loop out of `init()` into a new `plotMarkers(rows)` function. Replace the code in `init()` with a call to `plotMarkers`.

```javascript
// Add before init() function
function plotMarkers(rows) {
    // Clear existing markers
    markers.forEach(m => m.setMap(null));
    markers.length = 0;
    activeInfoWindow = null;

    document.getElementById('stat-loaded').textContent = rows.length.toString();

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
            if (activeInfoWindow) activeInfoWindow.close();
            infoWindow.open({ anchor: marker, map });
            activeInfoWindow = infoWindow;
        });
        
        markers.push(marker);
    }
}

// Update init() to use it (replace lines 105-132 with):
//      setStatus(`Plotting ${rows.length} lots...`, "loading");
//      plotMarkers(rows);
//      setStatus("Map Ready", "success");
```

- [ ] **Step 2: Add event listeners for Terapkan and Reset buttons**
Add the logic at the bottom of `init()` to read inputs, build SQL, query DuckDB, and plot.

```javascript
// Add at the very end of init() function (inside the try block)
        
        // Filter Logic
        document.getElementById('btn-apply-filters').addEventListener('click', async () => {
            setStatus("Applying Filters...", "loading");
            
            const cats = [];
            if (document.getElementById('cat-tanah').checked) cats.push("c.nama_lot_lelang ILIKE '%tanah%'");
            if (document.getElementById('cat-rumah').checked) cats.push("c.nama_lot_lelang ILIKE '%rumah%'");
            if (document.getElementById('cat-ruko').checked) cats.push("c.nama_lot_lelang ILIKE '%ruko%'");
            
            const minPrice = document.getElementById('price-min').value;
            const maxPrice = document.getElementById('price-max').value;
            const prov = document.getElementById('sel-provinsi').value;
            const kota = document.getElementById('sel-kota').value;

            let whereClauses = ["d.latitude IS NOT NULL AND d.longitude IS NOT NULL"];
            
            if (cats.length > 0) {
                whereClauses.push(`(${cats.join(' OR ')})`);
            }
            if (minPrice) whereClauses.push(`c.nilai_limit >= ${minPrice}`);
            if (maxPrice) whereClauses.push(`c.nilai_limit <= ${maxPrice}`);
            if (prov) whereClauses.push(`d.seller_provinsi = '${prov.replace(/'/g, "''")}'`);
            if (kota) whereClauses.push(`d.seller_kota = '${kota.replace(/'/g, "''")}'`);

            const sql = `
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
                WHERE ${whereClauses.join(' AND ')}
                LIMIT 500
            `;

            try {
                const res = await window.dbConn.query(sql);
                plotMarkers(res.toArray());
                setStatus("Filters Applied", "success");
            } catch (e) {
                setStatus("Filter Error", "error");
                console.error(e);
            }
        });

        document.getElementById('btn-reset-filters').addEventListener('click', async () => {
            document.getElementById('cat-tanah').checked = false;
            document.getElementById('cat-rumah').checked = false;
            document.getElementById('cat-ruko').checked = false;
            document.getElementById('price-min').value = '';
            document.getElementById('price-max').value = '';
            document.getElementById('sel-provinsi').value = '';
            document.getElementById('sel-kota').value = '';
            
            document.getElementById('btn-apply-filters').click();
        });
```

- [ ] **Step 3: Commit**

```bash
git add app.js
git commit -m "feat(filter): implement dynamic sql generation and map updating"
```