@echo off
REM apx work -- open the desktop window. Starts the service if it is not already up.
setlocal
cd /d "%~dp0..\.."
node "scripts\apocrypha-work\launch-gui.js" %*
