"""
Render a .mmd file to PNG via Mermaid.js in a headless browser.

Generates a self-contained HTML page with the Mermaid diagram,
opens it in Playwright, and takes a screenshot.

Usage:
    python dev_utils/render_mermaid.py <input.mmd> <output.png>
    python dev_utils/render_mermaid.py output/cc_arvo_57672/mermaid/goal-tree.mmd output/cc_arvo_57672/goal-tree.png
"""

import argparse
import sys
import tempfile
from pathlib import Path

HTML_TEMPLATE = """<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<script src="https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js"></script>
<style>
body {{ margin: 0; padding: 20px; background: white; }}
.mermaid {{ display: inline-block; }}
</style>
</head>
<body>
<pre class="mermaid">
{diagram}
</pre>
<script>
mermaid.initialize({{ startOnLoad: true, theme: 'default', securityLevel: 'loose' }});
</script>
</body>
</html>"""


def render(mmd_path: str, output_path: str, width: int = 1600, wait_ms: int = 3000):
    """Render .mmd file to PNG screenshot."""
    from playwright.sync_api import sync_playwright

    mmd_content = Path(mmd_path).read_text()

    html = HTML_TEMPLATE.format(diagram=mmd_content)

    with tempfile.NamedTemporaryFile(suffix=".html", mode="w", delete=False) as f:
        f.write(html)
        html_path = f.name

    try:
        with sync_playwright() as p:
            browser = p.chromium.launch()
            page = browser.new_page(viewport={"width": width, "height": 900})
            page.goto(f"file://{html_path}")
            page.wait_for_timeout(wait_ms)

            # Get the actual rendered size and screenshot just the diagram
            element = page.query_selector(".mermaid svg")
            if element:
                element.screenshot(path=output_path)
            else:
                page.screenshot(path=output_path, full_page=True)

            browser.close()
    finally:
        Path(html_path).unlink(missing_ok=True)

    print(f"✓ {output_path}")


def main():
    parser = argparse.ArgumentParser(
        description="Render .mmd to PNG via headless browser"
    )
    parser.add_argument("input", help="Input .mmd file")
    parser.add_argument("output", help="Output PNG path")
    parser.add_argument("--width", type=int, default=1600, help="Viewport width")
    parser.add_argument("--wait", type=int, default=3000, help="Wait time in ms")
    args = parser.parse_args()

    render(args.input, args.output, args.width, args.wait)


if __name__ == "__main__":
    main()
