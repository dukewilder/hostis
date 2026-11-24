@echo off
rem Starts Pyrate Radio and writes what it does to pyrate-log.txt in this folder.
rem The log names stations and radios: send it only to someone you trust, and delete it after.
set PYRATE_LOG=1
echo Pyrate Radio is running with a log. Leave this window open; it closes with the app.
"%~dp0pyrate.exe" 2> "%~dp0pyrate-log.txt"
