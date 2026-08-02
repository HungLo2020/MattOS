package cap

import (
	"bufio"
	"strings"
	"testing"
)

func TestFromTextRejectsScannerFailure(t *testing.T) {
	text := "cap_chown=e " + strings.Repeat("x", bufio.MaxScanTokenSize+1)

	got, err := FromText(text)
	if got != nil || err != ErrBadText {
		t.Fatalf("FromText() = (%v, %v), want (nil, %v)", got, err, ErrBadText)
	}
}
