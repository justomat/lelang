# Map Center Jakarta Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Center the Google Maps view on Jakarta by default instead of Indonesia.

**Architecture:** Modify the initial configuration object passed to `google.maps.Map` during initialization.

**Tech Stack:** JavaScript, Google Maps API

---

### Task 1: Update Map Initialization

**Files:**
- Modify: `app.js`

- [ ] **Step 1: Update coordinates and zoom in `app.js`**
Update the map initialization inside `app.js` (around line 158) to use Jakarta coordinates and zoom level 10.

```javascript
        map = new google.maps.Map(document.getElementById("map"), {
            center: { lat: -6.2088, lng: 106.8456 }, // Jakarta
            zoom: 10,
            mapId: "DEMO_MAP_ID",
            zoomControl: true,
            mapTypeControl: false,
            scaleControl: true,
            streetViewControl: false,
            rotateControl: false,
            fullscreenControl: true
        });
```

- [ ] **Step 2: Commit**

```bash
git add app.js
git commit -m "feat: center map on Jakarta with zoom level 10"
```
