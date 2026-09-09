@echo off
setlocal
set "APOCRYPHA_NODE_BASE_URL=https://www.apocky.com"
set "APOCRYPHA_CONTROLLER_KEY_ID=apocrypha-controller-prod-20260909"
node.exe dist\cli.js --network --opt-in
