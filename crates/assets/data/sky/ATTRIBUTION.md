# HYG 4.1 star catalogue attribution

Source: HYG Database v4.1 by Astronexus / David Nash, archived at
<https://github.com/astronexus/HYG-Database>.

Pinned input: `hyg/CURRENT/hygdata_v41.csv` at commit
`c7f7f883fe678cc7680169a50ccd7dcc49b060ce`. This asset selects the 1000
valid records with the lowest apparent visual magnitude, breaking exact ties
by HYG id. It excludes the Sun, removes unused columns, and converts the
selected records to the documented little-endian `stars-v1.bin` format.
Missing color index values use the documented neutral value 0.65; missing
proper-motion vectors are zero-filled. Numeric source coordinates are retained.
The aligned label asset uses HYG proper names where printable in the bundled
font, with HIP/HR/HYG identifiers as fallbacks.

The HYG data and this derived catalogue are licensed under
[Creative Commons Attribution-ShareAlike 4.0 International](https://creativecommons.org/licenses/by-sa/4.0/).
See `LICENSE-CC-BY-SA-4.0.txt` for the full license text and
`HYG-LICENSE-NOTICE.txt` for the exact upstream notice. Source license notice:
<https://github.com/astronexus/HYG-Database/blob/c7f7f883fe678cc7680169a50ccd7dcc49b060ce/LICENSE>.
