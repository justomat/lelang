# Frontend Map Filters & InfoWindow Design

## Goal
Improve the Google Maps frontend by fixing the InfoWindow behaviors (single open window, clickable title only) and introducing a filtering sidebar backed by DuckDB WASM for deep querying.

## Architecture

**Filtering Strategy (Approach B)**
We will use DuckDB WASM as the query engine for the filters rather than performing in-memory JavaScript filtering. When the user applies a filter, we dynamically construct a SQL `WHERE` clause and re-query the Parquet datasets to fetch the top 500 records that match the criteria. This guarantees that filtering reflects the entire database, not just the initially loaded slice.

## Components

### 1. InfoWindow Enhancements (`app.js`, `style.css`)
- **Single Open Window:** A global `activeInfoWindow` variable will track the currently open popup. Clicking a new marker will call `activeInfoWindow.close()` before opening the new one.
- **Link Scope:** The HTML template inside the `google.maps.InfoWindow` will be updated so that only the title (`<h3>`) is wrapped in an `<a>` tag with `target="_blank"`. The URL format is `https://lelang.go.id/lot-lelang/detail/<lot_lelang_id>`.
- **Close Button:** Ensure Google Maps' native close button (`x`) is visible by removing any conflicting CSS that might be hiding it.

### 2. Sidebar Filter UI (`index.html`, `style.css`)
- A new `<aside id="filter-sidebar" class="glass-panel">` fixed to the left side of the screen.
- **Category Checkboxes:** Three checkboxes for "Tanah", "Rumah", and "Ruko".
- **Price Range:** Two `<input type="number">` fields for "Min Harga" and "Max Harga".
- **Location Dropdowns:** Two `<select>` elements for "Provinsi" and "Kota/Kabupaten".
- **Action Buttons:** "Terapkan" (Apply) and "Reset Filter".

### 3. Dynamic Dropdown Population (`app.js`)
- During the `init()` sequence, immediately after instantiating DuckDB, execute two queries:
  1. `SELECT DISTINCT seller_provinsi FROM 'details.parquet' WHERE seller_provinsi IS NOT NULL AND seller_provinsi != '' ORDER BY seller_provinsi`
  2. `SELECT DISTINCT seller_kota FROM 'details.parquet' WHERE seller_kota IS NOT NULL AND seller_kota != '' ORDER BY seller_kota`
- Use the results to inject `<option>` tags into the respective dropdowns.

### 4. Dynamic Query Generation (`app.js`)
- Bind a click listener to the "Terapkan" button.
- Extract values from all form inputs.
- **SQL Construction:**
  - Base query remains a `LEFT JOIN` between `catalog.parquet` and `details.parquet`.
  - Categories: If checkboxes are selected, append `(c.nama_lot_lelang ILIKE '%tanah%' OR c.nama_lot_lelang ILIKE '%rumah%' ...)`
  - Price: If Min/Max are provided, append `AND c.nilai_limit >= MIN AND c.nilai_limit <= MAX`.
  - Location: If selected, append `AND d.seller_provinsi = '...'` and `AND d.seller_kota = '...'`.
- **Map Update:** Clear the `markers` array (calling `setMap(null)` on each), run the query, and plot the new set of markers. Update the "Lots Loaded" UI counter.
- **Reset:** The Reset button will clear all inputs and re-run the default (unfiltered) query.