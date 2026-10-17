"""Terminate only a disposable fixture worker at a transaction checkpoint."""
import json
import os
import sys

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.package import Package


def main():
    args = json.loads(sys.argv[1])
    library = Library(args["library"])
    def checkpoint(name):
        if name == args["checkpoint"]:
            os._exit(91)
    library.owned._checkpoint = checkpoint
    operation = args["operation"]
    if operation == "instance_migrate":
        import threading
        from creation_lib.mod_manager.controller import ManagerController
        from creation_lib.mod_manager.mo2_migration import migrate
        manager = ManagerController()
        try:
            migrate(args["instance"], library.root, manager._validate_destination, manager._prepare_library,
                    threading.Event(), lambda *args: None, checkpoint=checkpoint)
        finally:
            manager.cleanup()
    elif operation == "migrate":
        library.migrate(library.review_migration([args["mod_id"]]))
    elif operation == "uninstall":
        library.uninstall(library.review_owned(args["mod_id"]))
    elif operation == "restore":
        library.restore(args["history"])
    else:
        package = Package(args["package"])
        try:
            if operation == "reinstall":
                library.reinstall(library.review_owned(args["mod_id"]), package, package.plain_plan())
            else:
                library.install(package, package.plain_plan(), "Fixture", args["profile"])
        finally:
            package.close()


if __name__ == "__main__":
    main()
