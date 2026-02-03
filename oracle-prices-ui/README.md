# Oracle Prices UI

Web interface for viewing token prices from OutLayer oracle.

## Setup

1. Copy `.env.example` to `.env`:
```bash
cp .env.example .env
```

2. Edit `.env`:
```
API_URL=https://testnet-api.outlayer.fastnear.com
PROJECT_UUID=p0000000000000007
ASSETS=wrap.near,usdt.tether-token.near,aurora
PORT=8000
```

## Run

```bash
./start.sh
```

Open http://localhost:8000 in browser.

## Features

- Auto-refresh every 30 seconds
- Circular countdown indicator (bottom-right corner) - click to refresh now
- Batch API for efficient data fetching
- CORS proxy built-in
