#!/usr/bin/env bash
set -e

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$PROJECT_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

echo -e "${BOLD}${CYAN}"
echo "  ╔══════════════════════════════════════╗"
echo "  ║   Polymarket Arbitrage Dashboard     ║"
echo "  ╚══════════════════════════════════════╝"
echo -e "${RESET}"

# Cleanup on exit
cleanup() {
    echo -e "\n${BOLD}${RED}Shutting down...${RESET}"
    kill $API_PID $FRONTEND_PID 2>/dev/null
    wait $API_PID $FRONTEND_PID 2>/dev/null
    echo -e "${GREEN}Done.${RESET}"
}
trap cleanup EXIT INT TERM

# Check prerequisites
if ! command -v cargo &>/dev/null; then
    echo -e "${RED}Error: cargo not found. Install Rust: https://rustup.rs${RESET}"
    exit 1
fi

if ! command -v pnpm &>/dev/null && ! command -v npm &>/dev/null; then
    echo -e "${RED}Error: pnpm/npm not found. Install Node.js: https://nodejs.org${RESET}"
    exit 1
fi

# Install frontend deps if needed
if [ ! -d "frontend/node_modules" ]; then
    echo -e "${BLUE}[frontend]${RESET} Installing dependencies..."
    cd frontend
    if command -v pnpm &>/dev/null; then
        pnpm install
    else
        npm install
    fi
    cd ..
fi

# Start API server (background)
echo -e "${GREEN}[api]${RESET} Starting Axum server on ${BOLD}http://localhost:8080${RESET}"
cargo run -p arb-api 2>&1 | sed "s/^/$(printf "${GREEN}[api]${RESET} ")/" &
API_PID=$!

# Give the API a moment to start compiling
sleep 2

# Start frontend (background)
echo -e "${BLUE}[frontend]${RESET} Starting Next.js on ${BOLD}http://localhost:3000${RESET}"
cd frontend
if command -v pnpm &>/dev/null; then
    pnpm dev 2>&1 | sed "s/^/$(printf "${BLUE}[web]${RESET} ")/" &
else
    npm run dev 2>&1 | sed "s/^/$(printf "${BLUE}[web]${RESET} ")/" &
fi
FRONTEND_PID=$!
cd ..

echo -e ""
echo -e "${BOLD}${CYAN}  Dashboard: ${RESET}${BOLD}http://localhost:3000${RESET}"
echo -e "${BOLD}${CYAN}  API:       ${RESET}${BOLD}http://localhost:8080${RESET}"
echo -e "${BOLD}${CYAN}  Press Ctrl+C to stop${RESET}"
echo -e ""

# Wait for either to exit
wait -n $API_PID $FRONTEND_PID 2>/dev/null
