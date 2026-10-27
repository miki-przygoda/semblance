"""Fetch flyvis's pretrained model ensemble into the project's data/ directory.

flyvis's own CLI unpacks into site-packages, which is inside a gitignored venv
and would be destroyed by any `uv sync` rebuild. This calls the same function
with a destination we control, so the artifact survives.

Provenance is printed and written beside the download; anything exported from
these checkpoints becomes an input to a result and has to be traceable.
"""

import datetime
import json
import pathlib
import sys

from flyvis_cli.download_pretrained_models import download_and_unpack_files, checksums

# Both values are flyvis's own, copied from main() in
# flyvis_cli/download_pretrained_models.py (flyvis 1.2.0), where they are
# local variables and so cannot be imported. API_KEY is the public Google
# Drive API key flyvis ships for anyone downloading its pretrained models; it
# is not this project's credential.
FOLDER_ID = "15_8gPaVVJV6wGAspwkrdN8r-2NxMY4Kj"
API_KEY ="AIzaSyDOy2_N7B5UjxKy5Xxeyd9WZfnDWzQ4-54"

DEST = pathlib.Path(__file__).resolve().parent.parent / "data" / "flyvis-pretrained"


def main() -> int:
    DEST.mkdir(parents=True, exist_ok=True)
    print(f"destination: {DEST}")

    # skip_large_files=True drops results_umap_and_clustering.zip, which this
    # project has no use for.
    download_and_unpack_files(FOLDER_ID, str(DEST), API_KEY, skip_large_files=True)

    import flyvis

    provenance = {
        "fetched_utc": datetime.datetime.now(datetime.UTC).isoformat(timespec="seconds"),
        "flyvis_version": getattr(flyvis, "__version__", "unknown"),
        "drive_folder_id": FOLDER_ID,
        "expected_sha256": checksums,
        "note": "Fetched by tools/download_pretrained.py. Checksums verified by "
                "flyvis's own download_and_unpack_files.",
    }
    (DEST / "PROVENANCE.json").write_text(json.dumps(provenance, indent=2) + "\n")
    print(f"\nwrote {DEST / 'PROVENANCE.json'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
