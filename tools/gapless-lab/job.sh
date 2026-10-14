#!/bin/bash
# job.sh <id> <tracks-json>: publish a test and wait (up to 120 s) for its result from any lab client.
cd "$(dirname "$0")"
rm -f "result-$1.json"
printf '{"id":"%s","tracks":%s%s}' "$1" "$2" "${3:+,\"ref\":\"$3\"}" > job.json
for i in $(seq 1 120); do [ -f "result-$1.json" ] && break; sleep 1; done
cat "result-$1.json" 2>/dev/null || echo '{"error":"no result within 120 s"}'
