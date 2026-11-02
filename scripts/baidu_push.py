#!/usr/bin/env python3
"""Push built site URLs to Baidu's ziyuan "普通收录" API.

Reads the generated sitemap and POSTs every URL so Baidu indexes new and
updated pages without waiting for its crawler. Baidu allows 2,000 URLs per
day through this endpoint; we push the full sitemap (~40 URLs) on each
deploy, which is harmless for already-indexed URLs.

Usage (normally run from CI after the site is built):

    BAIDU_PUSH_TOKEN=<token> python3 scripts/baidu_push.py

The token comes from https://ziyuan.baidu.com -> 普通收录 -> 自动推送.
When the token is missing the script exits successfully without doing
anything, so the CI step is safe to keep unconditional.
"""

import os
import sys
import urllib.error
import urllib.request
import xml.etree.ElementTree as ET

SITE = "https://docs.phiagent.dev"
SITEMAP = "site/sitemap.xml"
API = "http://data.zz.baidu.com/urls?site={site}&token={token}"


def main() -> int:
    token = os.environ.get("BAIDU_PUSH_TOKEN", "").strip()
    if not token:
        print("BAIDU_PUSH_TOKEN not set, skipping Baidu push")
        return 0

    ns = {"sm": "http://www.sitemaps.org/schemas/sitemap/0.9"}
    root = ET.parse(SITEMAP).getroot()
    urls = [el.text for el in root.findall("sm:url/sm:loc", ns) if el.text]
    if not urls:
        print(f"no URLs found in {SITEMAP}, nothing to push")
        return 0

    api = API.format(site=SITE, token=token)
    req = urllib.request.Request(
        api,
        data="\n".join(urls).encode(),
        headers={"Content-Type": "text/plain"},
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            print(f"pushed {len(urls)} URLs:", resp.read().decode())
    except urllib.error.URLError as exc:
        print(f"Baidu push failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
