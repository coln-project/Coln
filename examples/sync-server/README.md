# Coln demo sync server

This private package provides the local, in-memory Subduction relay shared by
the current Coln demos.

Add it to a demo's development dependencies:

```json
{
  "devDependencies": {
    "@coln-project/demo-sync-server": "file:../sync-server"
  },
  "scripts": {
    "dev": "coln-with-sync-server vite --host 127.0.0.1",
    "server": "coln-sync-server"
  }
}
```

`coln-with-sync-server` reuses a compatible relay on port 3030 or starts one,
holds a lease while its child command runs, and sets both
`SUBDUCTION_ENDPOINT` and `VITE_SUBDUCTION_ENDPOINT`. This lets browser and Node
demos share one relay. Set `SUBDUCTION_PORT` to use another managed port.

`coln-sync-server` runs the relay directly. Set `PORT` and
`SUBDUCTION_SERVICE_NAME` to configure it. Data is held in memory and disappears
when the relay exits. A directly started relay remains running when demo leases
close and must be stopped explicitly.
