#!/bin/sed -rf
# CC0 cleanup code taken from papirus-icon-theme and modified for brevity.
# Processes SVG style attributes (not usefully, any more)

/style=/ {

	# add a trailing semicolon for secure matching
	s/style="([^"]+[^;])"/style="\1;"/gI

	# delete Inkscape properties
	/-inkscape-/ {
		s/-inkscape-[^;"]+;//gI
	}

	# delete properties with default values
	s/alignment-baseline:auto;//gI
	s/backface-visibility:visible;//gI

	# delete 'fill- *' properties if fill equal none
	/fill[:=]"?none/ {
		s/fill-rule:[^;"]+;//gI
		s/fill-opacity:[^;"]+;//gI
	}

	# delete a fill property if it has the default value and fill attribute not exist
	/fill="[^"]/! {
		s/fill:(#000|#000000|black);//gI
	}

	# delete a color property if currentColor not exists and fill has a value
	/currentColor/! {
		s/([^-])color:[^;"]+;/\1/gI
	}

	# delete 'stroke- *' properties if an object doesn't have a stroke
	/stroke[:=]/! {
		s/stroke-width:[^;"]+;//gI
		s/stroke-linecap:[^;"]+;//gI
	}
}
