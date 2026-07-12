# Plan: Google Maps Dashboard for Lelang Lots

## Objective
Replace the existing ECharts-based analytics dashboard with a new full-screen Google Maps dashboard. The new dashboard will load auction lots from the DuckDB Parquet files, extract the item addresses, geocode them into latitude/longitude coordinates using the Google Maps Geocoding API, and render them as markers on the map.

## File Modifications
- **`index.html`**: Completely replace the old dashboard UI. New UI will contain:
  - A full-screen `<div id="map"></div>`.
  - A floating UI panel for entering a Google Maps API Key (since we can't hardcode it).
  - A status/progress indicator for DuckDB initialization and Geocoding progress.
- **`style.css`**: Remove old grid and KPI styles. Add styles for a full-screen map, the API key modal, and custom Map InfoWindows.
- **`app.js`**: Remove ECharts logic and KPI calculations. Implement DuckDB initialization, Google Maps dynamic loading, address extraction, rate-limited geocoding, and map rendering.

## Step-by-Step Implementation Strategy

### 1. UI & Google Maps Initialization
1.  **API Key Modal:** On page load, check `localStorage` for a Google Maps API key. If not found, show a modal prompting the user to enter one.
2.  **Dynamic Script Loading:** Once the key is available, dynamically inject the Google Maps script tag (`https://maps.googleapis.com/maps/api/js?key=API_KEY&libraries=places`).
3.  **Map Setup:** Initialize a full-screen `google.maps.Map` centered on Indonesia.

### 2. Data Extraction via DuckDB WASM
1.  **Load Parquet:** Initialize DuckDB WASM and load `catalog_lots.parquet` and `lot_details.parquet` (as in the old dashboard).
2.  **Query Address Data:** Run a query to join both tables and extract the address from the JSON array in `lot_details.parquet`. If `barangs_json` is missing or empty, fallback to the general location (`nama_lokasi`).
    ```sql
    SELECT 
        c.lot_lelang_id, 
        c.nama_lot_lelang, 
        c.nilai_limit, 
        c.status,
        COALESCE(d.barangs_json->0->>'alamat', c.nama_lokasi) as address
    FROM 'catalog.parquet' c
    LEFT JOIN 'details.parquet' d ON c.lot_lelang_id = d.lot_lelang_id
    WHERE address IS NOT NULL AND address != ''
    LIMIT 200; -- Limit to avoid huge Geocoding API costs initially
    ```

### 3. Geocoding (Address to Lat/Long)
1.  **Local Cache:** Implement a `localStorage` cache (`address -> {lat, lng}`) to prevent hitting the Google Maps Geocoding API repeatedly for the same addresses across page reloads.
2.  **Rate-Limited Geocoding:** The Google Maps Geocoding API has strict rate limits. 
    - Create a queue of extracted addresses.
    - Process the queue sequentially using `google.maps.Geocoder.geocode()`.
    - Apply a delay (e.g., 200ms - 500ms) between requests to avoid `OVER_QUERY_LIMIT` errors.
3.  **Status Updates:** Update a progress bar on the UI showing "Geocoded X / Y Lots".

### 4. Rendering Markers
1.  **Map Markers:** As each address resolves to a Lat/Long, create a `google.maps.Marker` (or `AdvancedMarkerElement`) and add it to the map.
2.  **Interactivity:** Add click event listeners to the markers to open an `InfoWindow`. The window will display:
    - Lot Name (`nama_lot_lelang`)
    - Limit Value (`nilai_limit`) formatted as IDR.
    - Status (`status`)
    - The full extracted address.

### Notes for Execution
- Geocoding thousands of addresses purely in the browser can be slow and may incur Google Maps API costs. The initial implementation will include a reasonable `LIMIT` (e.g., 100-200 lots) in the SQL query. This limit can be adjusted or removed later by the user.
- We will instruct the user on how to obtain and input their API key.