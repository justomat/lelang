# Map Centering on Jakarta

## Purpose
Update the default Google Maps view to center on Jakarta with an appropriate zoom level instead of the current Indonesia-wide view.

## Design
- **Location:** Update map initialization in `app.js`.
- **Coordinates:** Change `center` to `{ lat: -6.2088, lng: 106.8456 }` (Jakarta).
- **Zoom Level:** Change `zoom` from `5` to `10` to provide a city-level metro view of Jakarta and its immediate surroundings.

## Trade-offs
- Setting a default zoom of 10 allows a good overview without being too far out or too close, meaning users can see properties across the wider Jakarta metropolitan area immediately.