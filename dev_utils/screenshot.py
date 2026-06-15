"""
Browser screenshot utility using Playwright.

Renders a URL in a headless browser and saves a screenshot.
Useful for capturing rendered Mermaid diagrams, React Flow charts, or any HTML.

Usage:
    python dev_utils/screenshot.py <url_or_html_file> <output.png> [--width 1200] [--height 800] [--wait 2000]
    python dev_utils/screenshot.py output/cc_arvo_57672/mermaid/goal-tree.html screenshot.png

Examples:
    # Screenshot a local HTML file
    python dev_utils/screenshot.py page.html output.png

    # Screenshot a running server
    python dev_utils/screenshot.py http://localhost:5173 output.png --width 1920 --height 1080

    # Wait for async rendering (ms)
    python dev_utils/screenshot.py page.html output.png --wait 3000
"""

import argparse
import sys
from pathlib import Path


def screenshot(
    target: str, output: str, width: int = 1200, height: int = 800, wait_ms: int = 2000
):
    """Render target in headless browser and save screenshot."""
    from playwright.sync_api import sync_playwright

    target_path = Path(target)
    if target_path.exists():
        url = f"file://{target_path.resolve()}"
    elif target.startswith("http"):
        url = target
    else:
        print(f"Error: '{target}' is not a valid URL or file path", file=sys.stderr)
        sys.exit(1)

    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": width, "height": height})
        page.goto(url)
        page.wait_for_timeout(wait_ms)
        page.screenshot(path=output, full_page=True)
        browser.close()

    print(f"✓ Screenshot saved: {output}")


def main():
    parser = argparse.ArgumentParser(description="Browser screenshot utility")
    parser.add_argument("target", help="URL or path to HTML file")
    parser.add_argument("output", help="Output PNG path")
    parser.add_argument(
        "--width", type=int, default=1200, help="Viewport width (default: 1200)"
    )
    parser.add_argument(
        "--height", type=int, default=800, help="Viewport height (default: 800)"
    )
    parser.add_argument(
        "--wait",
        type=int,
        default=2000,
        help="Wait time in ms for rendering (default: 2000)",
    )
    args = parser.parse_args()

    screenshot(args.target, args.output, args.width, args.height, args.wait)


if __name__ == "__main__":
    main()
