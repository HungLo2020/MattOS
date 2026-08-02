//go:build linux && go1.16
// +build linux,go1.16

package cap

import (
	"fmt"
	"os"
	"testing"
)

func TestFiles(t *testing.T) {
	old := GetProc()
	if present, err := old.GetFlag(Permitted, SETFCAP); err != nil {
		t.Fatalf("no file capability support: %v", err)
	} else if !present {
		t.Skip("no privilege for setting file capabilities")
	}
	elev, err := old.Dup()
	if err != nil {
		t.Fatalf("cannot duplicate [%v]: %v", old, err)
	}
	if err := elev.SetFlag(Effective, true, SETFCAP); err != nil {
		t.Fatalf("unable to effect SETFCAP: %v", err)
	}
	if err := elev.SetProc(); err != nil {
		t.Fatalf("unable to raise SETFCAP: %v", err)
	}
	defer old.SetProc()
	td := t.TempDir()
	reg := fmt.Sprint(td, "/reg")
	link := fmt.Sprint(td, "/sym")
	if err := os.WriteFile(reg, []byte("secure"), 0700); err != nil {
		t.Fatalf("unable to create target file %q: %v", reg, err)
	}
	if err := os.Symlink(reg, link); err != nil {
		t.Fatalf("unable to create target symlink %q to file %q: %v", link, reg, err)
	}
	if err := elev.SetFile(link); err == nil {
		t.Fatal("package supports applying file caps through symlinks!")
	}
	if err := elev.SetFile(reg); err != nil {
		t.Fatalf("unable to set file capability on %q: %v", reg, err)
	}
	if fc, err := GetFile(reg); err != nil {
		t.Errorf("unable to read file capability from %q: %v", reg, err)
	} else if diff, err := fc.Cf(elev); err != nil {
		t.Errorf("error comparing file capability got=[%v] want=[%v]: %v", fc, elev, err)
	} else if diff != 0 {
		t.Errorf("read capability [%v] != written one [%v] on %q", fc, elev, reg)
	}
	var lesser *Set
	if err := lesser.SetFile(reg); err != nil {
		t.Errorf("unable to remove file cap from %q: %v", reg, err)
	}

	if err := os.Chmod(reg, 0); err != nil {
		t.Fatalf("unable to drop access bits to %q: %v", reg, err)
	}
	lesser = NewSet()
	lesser.SetFlag(Permitted, true, SETFCAP)
	if err := lesser.SetFile(reg); err != nil {
		t.Fatalf("unable to set lesser capabilities [%v] on %q: %v", lesser, reg, err)
	}
	if fc, err := GetFile(reg); err != nil {
		t.Errorf("unable to read file capability from %q: %v", reg, err)
	} else if diff, err := fc.Cf(lesser); err != nil {
		t.Errorf("error comparing file capability got=[%v] want=[%v]: %v", fc, lesser, err)
	} else if diff != 0 {
		t.Errorf("read capability [%v] != written one [%v] on %q", fc, lesser, reg)
	}

	lesser = nil
	if err := lesser.SetFile(reg); err != nil {
		t.Errorf("unable to remove file cap from %q: %v", reg, err)
	}
}
