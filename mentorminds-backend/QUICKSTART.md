# Quick Start Guide - Backend Event Indexer

## Installation & Setup

### 1. Install Dependencies

```bash
cd mentorminds-backend
npm install
```

This will install all required packages:

- **express** - Web framework
- **socket.io** - WebSocket server
- **stellar-sdk** - Stellar blockchain SDK
- **cors** - Cross-origin resource sharing
- **dotenv** - Environment variables
- **pg** - PostgreSQL client
- **uuid** - Unique ID generation
- Plus all TypeScript dev dependencies

### 2. Configure Environment

Copy the example environment file:

```bash
cp .env.example .env
```

Edit `.env` and update:

- `HORIZON_URL` - Use testnet for development
- `FRONTEND_URL` - Your frontend URL (or keep `*`)
- Contract addresses after deployment

### 3. Development Mode

Run with auto-reload on changes:

```bash
npm run dev
```

You should see:

```
============================================================
MentorMinds Backend Server Started
============================================================
Environment: development
Port: 3001
REST API: http://localhost:3001
WebSocket: ws://localhost:3001/ws/events
Health Check: http://localhost:3001/health
============================================================
Starting Horizon event streaming...
[HorizonStream] Starting stream from cursor: now
```

### 4. Production Build

```bash
# Compile TypeScript
npm run build

# Run compiled code
npm start
```

---

## Verify Installation

### Test REST API

```bash
# Root endpoint
curl http://localhost:3001

# Health check
curl http://localhost:3001/health

# Get recent events
curl http://localhost:3001/api/events?limit=10

# Get cursor state
curl http://localhost:3001/api/events/cursor
```

### Test WebSocket

Open browser console or use Node.js:

```javascript
const socket = io("ws://localhost:3001/ws/events");

socket.on("connect", () => {
  console.log("Connected to WebSocket");

  // Subscribe to all events
  socket.emit("subscribe:all");
});

socket.on("event", (data) => {
  console.log("Received event:", data);
});
```

### Monitor Logs

Watch for these log messages:

```
[HorizonStream] Stream started
[EventIndexer] Saved event: created for contract CAAAA...
[WebSocket] Broadcasted event: created (ledger 123456)
```

---

## Troubleshooting

### Error: Cannot find module 'express'

**Solution**: Make sure you ran `npm install` in the correct directory

```bash
pwd  # Should end with mentorminds-backend
npm install
```

### Error: Port 3001 already in use

**Solution**: Change port in `.env`:

```bash
PORT=3002
```

### Horizon stream not starting

**Solution**: Check Horizon URL is accessible:

```bash
curl https://horizon-testnet.stellar.org/events?limit=1
```

### TypeScript compilation errors

**Solution**: These are normal before installing dependencies. After `npm install`, they will disappear.

---

## Next Steps

1. ✅ Install dependencies with `npm install`
2. ✅ Copy `.env.example` to `.env`
3. ✅ Run in dev mode: `npm run dev`
4. ✅ Test endpoints with curl or Postman
5. ✅ Update contract addresses in `.env` after deployment
6. ✅ Integrate with your frontend using WebSocket

---

## Available Scripts

```bash
npm run dev      # Development with auto-reload
npm run build    # Compile TypeScript
npm start        # Run compiled code
npm test         # Run tests
npm run lint     # Lint code
```

---

## Default Configuration

- **Server Port**: 3001
- **Horizon Network**: Stellar Testnet
- **CORS**: Open (`*`) - restrict in production
- **Auto-start Streaming**: Yes (2 second delay)

---

**Ready to Use!** 🚀

After installation, the service will automatically:

- Connect to Stellar Horizon
- Start streaming contract events
- Save events to memory
- Broadcast via WebSocket to subscribers
- Handle reconnection with exponential backoff

For detailed documentation, see [`EVENT-INDEXER-README.md`](./EVENT-INDEXER-README.md).
