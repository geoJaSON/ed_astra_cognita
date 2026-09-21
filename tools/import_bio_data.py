"""Converts EDMC-BioScan's and EDMC-ExploData's exobiology data into JSON for the app.

Both projects are GPL-2.0 (https://github.com/Silarn/EDMC-BioScan, https://github.com/Silarn/EDMC-ExploData).
Their Python modules are parsed with `ast` rather than imported, so no dependencies are needed.

Usage: python tools/import_bio_data.py [--bioscan REF] [--explodata REF]
Writes src-tauri/data/bio/*.json. Rerun when BioScan updates its rulesets.
"""

import argparse
import ast
import json
import urllib.request
from pathlib import Path

BIOSCAN_REF = "5f0d2e445a95681bf2e85223f883d5c552a7726b"  # 2026-07-11
EXPLODATA_REF = "fd4e188defd6405e46398fb0a54ac8669c9e2189"  # 2026-08-31

BIOSCAN_URL = "https://raw.githubusercontent.com/Silarn/EDMC-BioScan/{ref}/src/bio_scan/{path}"
EXPLODATA_URL = "https://raw.githubusercontent.com/Silarn/EDMC-ExploData/{ref}/src/ExploData/explo_data/{path}"

RULESET_MODULES = [
    "aleoida", "anemone", "bacterium", "brain_tree", "cactoida", "clypeus", "concha", "electricae", "fonticulua",
    "frutexa", "fumerola", "fungoida", "osseus", "recepta", "shard", "stratum", "tubers", "tubus", "tussock",
]

OUT_DIR = Path(__file__).resolve().parent.parent / "src-tauri" / "data" / "bio"


def fetch(url: str) -> str:
    with urllib.request.urlopen(url) as response:
        return response.read().decode("utf-8")


def assignments(source: str) -> dict:
    """Top-level `name = <literal>` assignments in a module, evaluated as literals."""
    values = {}
    for node in ast.parse(source).body:
        if isinstance(node, ast.Assign) and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name):
            name, value = node.targets[0].id, node.value
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name) and node.value is not None:
            name, value = node.target.id, node.value
        else:
            continue
        try:
            values[name] = ast.literal_eval(value)
        except ValueError:
            pass  # Not a literal (e.g. the `rules` merge expression); callers don't need these.
    return values


def write(name: str, data: dict) -> None:
    path = OUT_DIR / name
    path.write_text(json.dumps(data, separators=(",", ":"), ensure_ascii=False), encoding="utf-8")
    print(f"wrote {path.relative_to(OUT_DIR.parent.parent.parent)} ({path.stat().st_size // 1024} KiB)")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--bioscan", default=BIOSCAN_REF, help="EDMC-BioScan git ref")
    parser.add_argument("--explodata", default=EXPLODATA_REF, help="EDMC-ExploData git ref")
    args = parser.parse_args()

    def bioscan(path: str) -> dict:
        return assignments(fetch(BIOSCAN_URL.format(ref=args.bioscan, path=path)))

    def explodata(path: str) -> dict:
        return assignments(fetch(EXPLODATA_URL.format(ref=args.explodata, path=path)))

    source = {
        "bioscan": f"https://github.com/Silarn/EDMC-BioScan/tree/{args.bioscan}",
        "explodata": f"https://github.com/Silarn/EDMC-ExploData/tree/{args.explodata}",
        "license": "GPL-2.0",
    }

    # Species rulesets, keyed genus codex id -> species codex id.
    rules: dict = dict(bioscan("bio_data/species.py")["_mound_amphora"])
    for module in RULESET_MODULES:
        rules.update(bioscan(f"bio_data/rulesets/{module}.py")["catalog"])

    genus_info = explodata("bio_data/genus.py")["data"]
    genera = {}
    for genus_id, species in rules.items():
        info = genus_info.get(genus_id)
        if info is None:
            print(f"skipping {genus_id}: no genus data")  # BioScan skips these too.
            continue
        genera[genus_id] = {
            "name": info["name"],
            "colonyDistance": info["distance"],
            "colors": info.get("colors"),
            "species": species,
        }
    write("species.json", {"_source": source, "genera": genera})

    regions = bioscan("bio_data/regions.py")
    region_map = explodata("RegionMapData.py")
    write("regions.json", {
        "_source": source,
        "names": region_map["regions"],
        # Rows of [run length, region id] along x, one row per z step; see RegionMap.findRegion.
        "map": region_map["regionmap"],
        "groups": regions["region_map"],
        "guardianNebulae": regions["guardian_nebulae"],
        "tuberZones": regions["tuber_zones"],
    })

    nebulae = bioscan("nebula_data/reference_stars.py")
    sectors = bioscan("nebula_data/sectors.py")
    write("nebulae.json", {
        "_source": source,
        "large": list((nebulae["coordinates"] | nebulae["named_coordinates"]).values()),
        "planetary": list(nebulae["planetary_coordinates"].values()),
        "sectors": sectors["data"],
    })


if __name__ == "__main__":
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    main()
