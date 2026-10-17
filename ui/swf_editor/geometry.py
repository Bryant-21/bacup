"""Editor curve conversion with a measured pixel error and exact twip joins."""
from __future__ import annotations

from creation_lib.swf.shapes import CurvedEdge


def cubic_to_quadratic_records(p0, p1, p2, p3, tolerance=0.25):
    segments = []

    def approximate(a, b, c, d, depth=0):
        control = tuple((3 * (b[i] + c[i]) - a[i] - d[i]) / 4 for i in (0, 1))
        error = 0
        for t in (0.25, 0.75):
            cubic = tuple((1 - t)**3 * a[i] + 3 * (1 - t)**2 * t * b[i]
                          + 3 * (1 - t) * t*t * c[i] + t**3 * d[i] for i in (0, 1))
            quad = tuple((1 - t)**2 * a[i] + 2 * (1 - t) * t * control[i] + t*t * d[i] for i in (0, 1))
            error = max(error, sum((cubic[i] - quad[i])**2 for i in (0, 1))**0.5)
        if error <= tolerance or depth >= 16:
            segments.append((control, d))
            return
        def midpoint(x, y):
            return tuple((x[i] + y[i]) / 2 for i in (0, 1))
        ab, bc, cd = midpoint(a, b), midpoint(b, c), midpoint(c, d)
        abc, bcd = midpoint(ab, bc), midpoint(bc, cd)
        middle = midpoint(abc, bcd)
        approximate(a, ab, abc, middle, depth + 1)
        approximate(middle, bcd, cd, d, depth + 1)

    approximate(p0, p1, p2, p3)
    previous = tuple(round(value * 20) for value in p0)
    records = []
    for control, end in segments:
        c, e = [tuple(round(value * 20) for value in point) for point in (control, end)]
        records.append(CurvedEdge(cx=c[0] - previous[0], cy=c[1] - previous[1], ax=e[0] - c[0], ay=e[1] - c[1]))
        previous = e
    return records
