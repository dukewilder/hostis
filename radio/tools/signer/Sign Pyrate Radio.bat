@echo off
rem Opens the Pyrate Radio signing window.
powershell -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%~dp0signer.ps1"
