#!/usr/bin/env python3
"""Writes the assetlinks.json that lets hostis.black links open Pyrate Radio directly.

  assetlinks.py android-cert.der > assetlinks.json

The certificate is the public half Dylan sends (never the key). The file belongs at
https://hostis.black/.well-known/assetlinks.json, served as application/json.
"""
import hashlib, json, sys

der = open(sys.argv[1], "rb").read()
fp = ":".join(f"{b:02X}" for b in hashlib.sha256(der).digest())
print(json.dumps([{
    "relation": ["delegate_permission/common.handle_all_urls"],
    "target": {"namespace": "android_app", "package_name": "black.hostis.pyrate", "sha256_cert_fingerprints": [fp]},
}], indent=2))
