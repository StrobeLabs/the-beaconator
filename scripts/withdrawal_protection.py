#!/usr/bin/env python3
"""Read or prepare withdrawal protection without the web app; never signs."""
import argparse
import json
import os
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=["status", "pause"])
    parser.add_argument("--targets", type=Path, required=True,
                        help="Saved registry export, or a list of ControllerTarget objects")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    url = os.environ.get("BEACONATOR_ADDRESS")
    token = os.environ.get("BEACONATOR_ACCESS_TOKEN")
    if not url or not token:
        parser.error("Set this environment's BEACONATOR_ADDRESS and BEACONATOR_ACCESS_TOKEN")
    rows = json.loads(args.targets.read_text())
    targets = [row.get("target", row) for row in rows]
    payload = {"targets": targets}
    endpoint = "status"
    if args.operation == "pause":
        endpoint = "prepare"
        payload["operation"] = {"action": "pause"}
    request = Request(url.rstrip("/") + "/withdrawals/" + endpoint,
                      data=json.dumps(payload).encode(), method="POST",
                      headers={"Authorization": "Bearer " + token,
                               "Content-Type": "application/json"})
    try:
        with urlopen(request, timeout=120) as response:
            result = json.load(response)
    except (HTTPError, URLError):
        parser.exit(1, "Controller verification failed; no batch was prepared.\n")
    # Exclusive create avoids overwriting an incident's earlier evidence.
    with args.output.open("x") as output:
        json.dump(result, output, indent=2)
        output.write("\n")
    if endpoint == "prepare":
        for index, group in enumerate(result["groups"]):
            path = args.output.with_name(args.output.stem + f"-safe-{index}.json")
            with path.open("x") as output:
                json.dump(group["transaction_builder"], output, indent=2)
                output.write("\n")
    print("Saved verified snapshot/export. No transaction was signed or executed.")


if __name__ == "__main__":
    main()
