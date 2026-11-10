# Points the app's links at the open player just built (browser/www/open.json), before an app
# is built: the deploy then publishes that player as well as the newest, so the links in every
# app keep working. Run after browser/build.sh and before the APK or the computers' builds.
import json, os
R = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
op = json.load(open(f"{R}/browser/www/open.json"))
open(f"{R}/app/assets/open_players.txt", "w").write("\n".join(op["players"]) + "\n")
print("the app names", op["cid"])
