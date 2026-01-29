#!/bin/bash
set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

command -v jq >/dev/null 2>&1 || { echo "jq is required"; exit 1; }

COMPOSE_FILE="$(dirname "$0")/../compose/devnet.yaml"
REFRESH_INTERVAL=${1:-0.5}
CHAIN_ID="${CHAIN_ID:-1337}"

cleanup() {
    tput cnorm
    echo ""
    exit 0
}
trap cleanup INT TERM

format_uptime() {
    local seconds=$1
    if [[ $seconds -ge 86400 ]]; then
        printf "%dd%dh" $((seconds/86400)) $((seconds%86400/3600))
    elif [[ $seconds -ge 3600 ]]; then
        printf "%dh%dm" $((seconds/3600)) $((seconds%3600/60))
    elif [[ $seconds -ge 60 ]]; then
        printf "%dm%ds" $((seconds/60)) $((seconds%60))
    else
        printf "%ds" $seconds
    fi
}

get_container_uptime() {
    local container=$1
    # Get uptime directly from docker in seconds
    local running=$(docker inspect --format '{{.State.Running}}' "$container" 2>/dev/null)
    if [[ "$running" == "true" ]]; then
        # Use docker's own calculation
        local started=$(docker inspect --format '{{.State.StartedAt}}' "$container" 2>/dev/null)
        # Convert to epoch using python (most reliable cross-platform)
        local start_epoch=$(python3 -c "from datetime import datetime; print(int(datetime.fromisoformat('${started%Z}'.replace('Z','+00:00')).timestamp()))" 2>/dev/null || echo 0)
        local now=$(date +%s)
        echo $((now - start_epoch))
    else
        echo 0
    fi
}

# Extract consensus state from logs
get_consensus_state() {
    local container=$1
    local logs=$(docker logs --tail 200 "$container" 2>&1)
    
    # Get the highest view number seen
    local latest_view=$(echo "$logs" | grep -oE 'view: View\([0-9]+\)' | grep -oE '[0-9]+' | sort -n | tail -1)
    [[ -z "$latest_view" ]] && latest_view=0
    
    # Count unique finalization broadcasts (actual consensus progress)
    local finalizations=$(echo "$logs" | grep -c "Finalization" 2>/dev/null || echo 0)
    
    # Count nullifications (view changes without finalization)
    local nullifications=$(echo "$logs" | grep -c "nullification" 2>/dev/null || echo 0)
    
    # Count proposals seen
    local proposals=$(echo "$logs" | grep -c "proposal:" 2>/dev/null || echo 0)
    
    echo "$latest_view|$proposals|$finalizations|$nullifications"
}

render() {
    tput cup 0 0
    
    local now=$(date "+%H:%M:%S")
    
    echo -e "${BOLD}${BLUE}╔══════════════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}${BLUE}║${NC}                       ${BOLD}KORA DEVNET MONITOR${NC}                                ${BOLD}${BLUE}║${NC}"
    echo -e "${BOLD}${BLUE}╚══════════════════════════════════════════════════════════════════════════╝${NC}"
    echo -e "  ${DIM}$now${NC}  │  ${DIM}Chain:${NC} ${CYAN}$CHAIN_ID${NC}  │  ${DIM}Refresh:${NC} ${REFRESH_INTERVAL}s  │  ${DIM}Ctrl+C to exit${NC}"
    echo ""
    
    local containers=$(docker compose -f "$COMPOSE_FILE" ps --format json 2>/dev/null)
    
    # Consensus state section
    echo -e "${BOLD}${CYAN}Consensus State${NC}"
    echo -e "┌───────┬──────────┬─────────┬───────────┬────────────┬────────────┐"
    echo -e "│ ${BOLD}Node${NC}  │ ${BOLD}Status${NC}   │ ${BOLD}View${NC}    │ ${BOLD}Proposals${NC} │ ${BOLD}Finalized${NC}  │ ${BOLD}Nullified${NC}  │"
    echo -e "├───────┼──────────┼─────────┼───────────┼────────────┼────────────┤"
    
    local healthy_count=0
    local max_view=0
    local total_finalized=0
    
    for i in 0 1 2 3; do
        local service="validator-node$i"
        local container="kora-devnet-$service-1"
        
        local health=$(echo "$containers" | jq -r "select(.Service == \"$service\") | .Health" 2>/dev/null)
        local state=$(echo "$containers" | jq -r "select(.Service == \"$service\") | .State" 2>/dev/null)
        
        if [[ "$state" == "running" ]]; then
            local consensus=$(get_consensus_state "$container")
            local view=$(echo "$consensus" | cut -d'|' -f1)
            local proposals=$(echo "$consensus" | cut -d'|' -f2)
            local finalized=$(echo "$consensus" | cut -d'|' -f3)
            local nullified=$(echo "$consensus" | cut -d'|' -f4)
            
            [[ -z "$view" ]] && view=0
            [[ $view -gt $max_view ]] && max_view=$view
            total_finalized=$((total_finalized + finalized))
            
            if [[ "$health" == "healthy" ]]; then
                local status="${GREEN}healthy${NC}  "
                ((healthy_count++))
            elif [[ "$health" == "starting" ]]; then
                local status="${YELLOW}starting${NC} "
            else
                local status="${YELLOW}waiting${NC}  "
            fi
            
            printf "│ ${CYAN}%-5s${NC} │ %b│ %-7s │ %-9s │ %-10s │ %-10s │\n" \
                "$i" "$status" "$view" "$proposals" "$finalized" "$nullified"
        else
            printf "│ ${CYAN}%-5s${NC} │ ${RED}stopped${NC}  │ -       │ -         │ -          │ -          │\n" "$i"
        fi
    done
    
    echo -e "└───────┴──────────┴─────────┴───────────┴────────────┴────────────┘"
    
    # Chain summary
    echo ""
    echo -e "${BOLD}${CYAN}Chain Summary${NC}"
    
    local health_color=$GREEN
    [[ $healthy_count -lt 4 ]] && health_color=$YELLOW
    [[ $healthy_count -lt 3 ]] && health_color=$RED
    
    local threshold_status="${GREEN}✓ Met${NC}"
    [[ $healthy_count -lt 3 ]] && threshold_status="${RED}✗ Not met${NC}"
    
    # Determine leader (view mod 4 for round-robin)
    local leader=$((max_view % 4))
    
    echo -e "  ${DIM}Validators:${NC} ${health_color}${healthy_count}/4${NC}    ${DIM}Threshold:${NC} $threshold_status    ${DIM}Current View:${NC} ${BOLD}$max_view${NC}    ${DIM}Leader:${NC} ${MAGENTA}node$leader${NC}"
    
    # Calculate uptime from node0
    local uptime_secs=$(get_container_uptime "kora-devnet-validator-node0-1")
    local uptime="0s"
    [[ $uptime_secs -gt 0 ]] && uptime=$(format_uptime "$uptime_secs")
    
    # Estimate views per second
    local vps="-"
    if [[ $uptime_secs -gt 0 && $max_view -gt 0 ]]; then
        vps=$(awk "BEGIN {printf \"%.1f\", $max_view / $uptime_secs}" 2>/dev/null || echo "-")
    fi
    
    local avg_final=$((total_finalized / 4))
    echo -e "  ${DIM}Uptime:${NC} $uptime    ${DIM}Views/sec:${NC} ${vps}    ${DIM}Avg Finalized:${NC} $avg_final"
    
    # Endpoints
    echo ""
    echo -e "${BOLD}${CYAN}Endpoints${NC}"
    echo -e "  ${DIM}P2P:${NC} 30400-30403    ${DIM}Metrics:${NC} 9000-9003"
    
    local prom_running=$(echo "$containers" | jq -r 'select(.Service == "prometheus") | .State' 2>/dev/null)
    local grafana_running=$(echo "$containers" | jq -r 'select(.Service == "grafana") | .State' 2>/dev/null)
    
    if [[ "$prom_running" == "running" ]]; then
        printf "  ${DIM}Prometheus:${NC} ${GREEN}localhost:9090${NC}"
    else
        printf "  ${DIM}Prometheus:${NC} ${DIM}off${NC}"
    fi
    
    if [[ "$grafana_running" == "running" ]]; then
        echo -e "    ${DIM}Grafana:${NC} ${GREEN}localhost:3000${NC}"
    else
        echo -e "    ${DIM}Grafana:${NC} ${DIM}off${NC}"
    fi
    
    # Live activity stream
    echo ""
    echo -e "${BOLD}${CYAN}Live Activity${NC}"
    echo -e "┌────────────────────────────────────────────────────────────────────────────┐"
    
    for i in 0 1 2 3; do
        local container="kora-devnet-validator-node$i-1"
        # Get last meaningful log line (skip buffer warnings)
        local last_log=$(docker logs --tail 50 "$container" 2>&1 | grep -v "buffer capacity" | grep -v "^\s*$" | tail -1 || echo "-")
        # Strip ANSI codes and timestamp
        last_log=$(echo "$last_log" | LC_ALL=C tr -d '\000-\037' | sed 's/^[0-9T:.Z-]* *//' | sed 's/  */ /g')
        # Truncate to fixed width
        printf "│ ${CYAN}%d${NC} %-72.72s │\n" "$i" "$last_log"
    done
    
    echo -e "└────────────────────────────────────────────────────────────────────────────┘"
    
    # Clear extra lines
    for _ in {1..2}; do
        printf "%-78s\n" ""
    done
}

# Main
clear
tput civis

render

while true; do
    sleep "$REFRESH_INTERVAL"
    render
done
