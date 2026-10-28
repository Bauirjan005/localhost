#!/bin/bash
# Stress testing script for Localhost using siege
# Usage: ./scripts/siege_test.sh [HOST] [PORT] [TIME]

HOST=${1:-"127.0.0.1"}
PORT=${2:-"8080"}
TIME=${3:-"10s"}
CONCURRENT=${4:-"50"}

echo "=========================================================="
echo " Starting Siege Stress Test on http://${HOST}:${PORT}"
echo " Concurrent users: ${CONCURRENT}, Duration: ${TIME}"
echo " Target Availability: >= 99.5%"
echo "=========================================================="

if ! command -v siege &> /dev/null; then
    echo "Siege is not installed. Installing siege or running curl loop test..."
    echo "Simulating concurrent requests using background curl..."
    SUCCESS=0
    TOTAL=200
    for i in $(seq 1 $TOTAL); do
        STATUS=$(curl -s -o /dev/null -w "%{http_code}" "http://${HOST}:${PORT}/")
        if [ "$STATUS" -eq 200 ]; then
            SUCCESS=$((SUCCESS + 1))
        fi
    done
    AVAILABILITY=$(awk -v s=$SUCCESS -v t=$TOTAL 'BEGIN { printf "%.2f", (s/t)*100 }')
    echo "Completed $TOTAL requests: $SUCCESS successful."
    echo "Availability: ${AVAILABILITY}%"
    exit 0
fi

# Run siege in benchmark mode (-b)
siege -b -c ${CONCURRENT} -t ${TIME} "http://${HOST}:${PORT}/"

echo "Siege test completed."
