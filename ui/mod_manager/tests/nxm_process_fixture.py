"""Disposable manager process controlled through stdin; never connects or downloads."""
import argparse
import json
from pathlib import Path
import queue
import sys

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.nxm_instances import Inbox
from creation_lib.mod_manager.nxm_routing import context, validate


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--library", type=Path, required=True)
    parser.add_argument("--name", required=True)
    parser.add_argument("--game", default="fallout4")
    args = parser.parse_args()
    manager = ManagerController()
    manager.library = Library(args.library, create=True)
    def profile(game):
        identifier = manager.library.create_profile(args.name + " " + game)
        with manager.library._db() as db:
            db.execute("UPDATE profiles SET files=? WHERE id=?", (json.dumps({"_modbox_game": game}), identifier))
        manager.profile_id = identifier
        manager.refresh()
    profile(args.game)
    inbox = Inbox(args.directory, context=context(manager))
    def reply(value):
        print(json.dumps(value), flush=True)
    reply({"ready": True, "instance_id": inbox.instance_id})
    try:
        for line in sys.stdin:
            command = json.loads(line)
            if command["command"] == "close":
                reply({"closed": True})
                break
            if command["command"] == "switch":
                profile(command["game"])
                inbox.publish(context(manager))
            if command["command"] == "drain":
                while True:
                    try:
                        request = inbox.requests.get_nowait()
                    except queue.Empty:
                        break
                    try:
                        value = validate(manager, request.destination.context)
                        manager.accept_nxm(request.link, destination=value)
                        inbox.mark(request, "delivered")
                    except ValueError:
                        inbox.mark(request, "stale")
            reply({"queued": inbox.requests.qsize(), "pending": [list(i) for i in manager.pending_links],
                   "bindings": [list(v) if v else None for v in manager.pending_destinations.values()],
                   "downloads": len(manager.library.downloads()), "context": context(manager)})
    finally:
        inbox.close()
        manager.cleanup()


if __name__ == "__main__":
    main()
