package conformance

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"net/http"
	"os"
	"os/exec"
	"strings"
	"testing"

	"sigs.k8s.io/external-dns/endpoint"
	externaldns "sigs.k8s.io/external-dns/pkg/apis/externaldns"
	"sigs.k8s.io/external-dns/plan"
	"sigs.k8s.io/external-dns/provider/webhook"
	"sigs.k8s.io/external-dns/registry/txt"
)

func TestRustWebhookConvergesWithExternalDNS(t *testing.T) {
	binary := os.Getenv("RUST_BINARY")
	if binary == "" {
		t.Fatal("set RUST_BINARY to the compiled Rust executable")
	}
	baseURL, fake := routerOSTarget(t)
	port := reservePort(t)
	cmd := exec.Command(binary)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	cmd.Env = append(os.Environ(), "MIKROTIK_BASEURL="+baseURL,
		"MIKROTIK_USERNAME="+routerOSUsername(t), "MIKROTIK_PASSWORD="+routerOSPassword(t), "SERVER_HOST=127.0.0.1",
		"SERVER_PORT="+port, "MIKROTIK_DEFAULT_TTL=3600")
	if err := cmd.Start(); err != nil {
		t.Fatalf("start Rust webhook: %v", err)
	}
	t.Cleanup(func() { _ = cmd.Process.Kill(); _, _ = cmd.Process.Wait() })
	base := "http://127.0.0.1:" + port
	waitReady(t, base+"/records")

	cfg := externaldns.NewConfig()
	cfg.WebhookProviderURL = base
	var nonce [8]byte
	if _, err := rand.Read(nonce[:]); err != nil {
		t.Fatal(err)
	}
	name := "conformance-" + hex.EncodeToString(nonce[:]) + ".example.org"
	ownerID := "conformance-" + hex.EncodeToString(nonce[:])
	cfg.TXTOwnerID = ownerID
	cfg.ManagedDNSRecordTypes = []string{endpoint.RecordTypeA, endpoint.RecordTypeAAAA, endpoint.RecordTypeCNAME, endpoint.RecordTypeTXT, endpoint.RecordTypeMX, endpoint.RecordTypeSRV, endpoint.RecordTypeNS}
	provider, err := webhook.New(context.Background(), cfg, nil)
	if err != nil {
		t.Fatal(err)
	}
	registry, err := txt.New(cfg, provider)
	if err != nil {
		t.Fatal(err)
	}
	desired := endpoint.NewEndpointWithTTL(name, endpoint.RecordTypeA, 0, "192.0.2.10")
	desired.WithProviderSpecific("comment", "managed")
	if !converge(t, registry, ownerID, desired) {
		t.Fatal("initial desired state produced no planner changes")
	}
	writesAfterCreate := 0
	if fake != nil {
		writesAfterCreate = fake.writeCount()
	}
	if converge(t, registry, ownerID, desired) {
		t.Fatal("first replay produced planner changes")
	}
	if converge(t, registry, ownerID, desired) {
		t.Fatal("second replay produced planner changes")
	}
	if fake != nil && fake.writeCount() != writesAfterCreate {
		t.Fatal("no-change planner cycles wrote to RouterOS")
	}

	if fake != nil {
		records := fake.snapshot()
		if len(records) != 2 {
			t.Fatalf("expected A and ownership TXT records, got %#v", records)
		}
		assertProperties(t, records, ownerID, name)
	}
	var originalID string
	if fake != nil {
		originalID = recordID(fake.snapshot(), "192.0.2.10")
	}

	multi := endpoint.NewEndpointWithTTL(name, endpoint.RecordTypeA, 0, "192.0.2.10", "192.0.2.11")
	multi.WithProviderSpecific("comment", "managed")
	if !converge(t, registry, ownerID, multi) {
		t.Fatal("multi-target update produced no planner changes")
	}
	if fake != nil {
		records := fake.snapshot()
		if len(records) != 3 || !containsAddress(records, "192.0.2.10") || !containsAddress(records, "192.0.2.11") {
			t.Fatalf("multi-target union did not persist: %#v", records)
		}
		if recordID(records, "192.0.2.10") != originalID {
			t.Fatalf("multi-target update replaced the existing target ID: %#v", records)
		}
	}
	if converge(t, registry, ownerID, multi) {
		t.Fatal("multi-target replay produced planner changes")
	}
	updated := endpoint.NewEndpointWithTTL(name, endpoint.RecordTypeA, 0, "192.0.2.10", "192.0.2.11")
	updated.WithProviderSpecific("comment", "changed")
	if !converge(t, registry, ownerID, updated) {
		t.Fatal("metadata update produced no planner changes")
	}
	if fake != nil && !containsAddress(fake.snapshot(), "192.0.2.11") {
		t.Fatalf("target update did not reach RouterOS: %#v", fake.snapshot())
	}
	if fake != nil {
		for _, record := range fake.snapshot() {
			if record.Type == endpoint.RecordTypeA && !strings.HasSuffix(record.Comment, ";changed") {
				t.Fatalf("metadata update did not persist user comment: %#v", record)
			}
		}
	}
	if converge(t, registry, ownerID, updated) {
		t.Fatal("post-update replay produced planner changes")
	}
	if fake != nil {
		fake.drift("192.0.2.10", "30m")
		if !converge(t, registry, ownerID, updated) {
			t.Fatal("physical TTL drift produced no planner changes")
		}
		if fake.snapshot()[0].TTL != "1h" {
			t.Fatalf("physical TTL drift was not repaired: %#v", fake.snapshot())
		}
	}

	second := endpoint.NewEndpoint("second."+name, endpoint.RecordTypeA, "192.0.2.12")
	if !converge(t, registry, ownerID, updated, second) {
		t.Fatal("second record produced no planner changes")
	}
	if converge(t, registry, ownerID, updated, second) {
		t.Fatal("second record replay produced planner changes")
	}
	// Given six additional types and their RouterOS representations; when the real
	// planner applies them; then the next observation must be stable.
	types := []*endpoint.Endpoint{
		endpoint.NewEndpointWithTTL("ipv6."+name, endpoint.RecordTypeAAAA, 300, "2001:db8::1"),
		endpoint.NewEndpointWithTTL("alias."+name, endpoint.RecordTypeCNAME, 300, "target.example.org"),
		endpoint.NewEndpointWithTTL("text."+name, endpoint.RecordTypeTXT, 300, ""),
		endpoint.NewEndpointWithTTL("mail."+name, endpoint.RecordTypeMX, 300, "10 mail.example.org"),
		endpoint.NewEndpointWithTTL("_service._tcp."+name, endpoint.RecordTypeSRV, 300, "1 2 443 service.example.org"),
		endpoint.NewEndpointWithTTL("ns."+name, endpoint.RecordTypeNS, 300, "ns1.example.org"),
	}
	all := append([]*endpoint.Endpoint{updated, second}, types...)
	if !converge(t, registry, ownerID, all...) {
		t.Fatal("seven-type creation produced no planner changes")
	}
	if converge(t, registry, ownerID, all...) {
		t.Fatal("seven-type replay produced planner changes")
	}
	assertPhysicalTypes(t, baseURL, fake, name)
	if fake != nil {
		fake.injectReadError()
		if _, err := registry.Records(context.Background()); err == nil {
			t.Fatal("RouterOS read error was not propagated through the real webhook client")
		}
		fake.addForeign(rosRecord{Name: "foreign." + name, Type: "A", Address: "192.0.2.200", TTL: "3600s"})
	}
	if err := deleteAll(t, registry, ownerID); err != nil {
		t.Fatal(err)
	}
	if fake != nil {
		records := fake.snapshot()
		if len(records) != 1 || records[0].Address != "192.0.2.200" {
			t.Fatalf("delete changed a foreign record or left owned records: %#v", records)
		}
	}
}

func assertPhysicalTypes(t *testing.T, baseURL string, fake *routerOSFake, name string) {
	t.Helper()
	seen := map[string]bool{}
	if fake != nil {
		for _, record := range fake.snapshot() {
			if strings.Contains(record.Name, name) {
				seen[record.Type] = true
			}
		}
	} else {
		apiURL := strings.TrimRight(baseURL, "/") + "/rest/ip/dns/static"
		req, err := http.NewRequest(http.MethodGet, apiURL, nil)
		if err != nil {
			t.Fatal(err)
		}
		req.SetBasicAuth(routerOSUsername(t), routerOSPassword(t))
		response, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		defer response.Body.Close()
		var records []rosRecord
		if err := json.NewDecoder(response.Body).Decode(&records); err != nil {
			t.Fatal(err)
		}
		for _, record := range records {
			if strings.Contains(record.Name, name) {
				seen[record.Type] = true
			}
		}
	}
	for _, recordType := range []string{"A", "AAAA", "CNAME", "TXT", "MX", "SRV", "NS"} {
		if !seen[recordType] {
			t.Errorf("physical %s record missing", recordType)
		}
	}
}

func converge(t *testing.T, registry interface {
	Records(context.Context) ([]*endpoint.Endpoint, error)
	AdjustEndpoints([]*endpoint.Endpoint) ([]*endpoint.Endpoint, error)
	ApplyChanges(context.Context, *plan.Changes) error
}, ownerID string, desired ...*endpoint.Endpoint) bool {
	t.Helper()
	ctx := context.Background()
	current, err := registry.Records(ctx)
	if err != nil {
		t.Fatal(err)
	}
	adjusted, err := registry.AdjustEndpoints(desired)
	if err != nil {
		t.Fatal(err)
	}
	calculated := (&plan.Plan{Current: current, Desired: adjusted, OwnerID: ownerID, ManagedRecords: []string{endpoint.RecordTypeA, endpoint.RecordTypeAAAA, endpoint.RecordTypeCNAME, endpoint.RecordTypeTXT, endpoint.RecordTypeMX, endpoint.RecordTypeSRV, endpoint.RecordTypeNS}}).Calculate()
	if calculated.Changes == nil {
		t.Fatal("planner returned nil changes")
	}
	if !calculated.Changes.HasChanges() {
		return false
	}
	t.Logf("planned changes: create=%v updateOld=%v updateNew=%v delete=%v", calculated.Changes.Create, calculated.Changes.UpdateOld, calculated.Changes.UpdateNew, calculated.Changes.Delete)
	if err := registry.ApplyChanges(ctx, calculated.Changes); err != nil {
		t.Fatal(err)
	}
	return true
}

func deleteAll(t *testing.T, registry interface {
	Records(context.Context) ([]*endpoint.Endpoint, error)
	AdjustEndpoints([]*endpoint.Endpoint) ([]*endpoint.Endpoint, error)
	ApplyChanges(context.Context, *plan.Changes) error
}, ownerID string) error {
	current, err := registry.Records(context.Background())
	if err != nil {
		return err
	}
	changes := (&plan.Plan{Current: current, Desired: nil, OwnerID: ownerID, ManagedRecords: []string{endpoint.RecordTypeA, endpoint.RecordTypeAAAA, endpoint.RecordTypeCNAME, endpoint.RecordTypeTXT, endpoint.RecordTypeMX, endpoint.RecordTypeSRV, endpoint.RecordTypeNS}}).Calculate().Changes
	if !changes.HasChanges() {
		return nil
	}
	return registry.ApplyChanges(context.Background(), changes)
}

func assertProperties(t *testing.T, records []rosRecord, ownerID, name string) {
	seenTXT := false
	for _, r := range records {
		if r.Type == endpoint.RecordTypeTXT {
			seenTXT = true
			if !strings.Contains(r.Text, "heritage=external-dns") || !strings.Contains(r.Text, "owner="+ownerID) {
				t.Errorf("ownership TXT missing registry metadata: %#v", r)
			}
		}
		if strings.Contains(r.Name, name) && r.TTL == "0s" {
			t.Errorf("TTL 0 was persisted: %#v", r)
		}
	}
	if !seenTXT {
		t.Error("no ownership TXT record was persisted")
	}
}
func containsAddress(records []rosRecord, address string) bool {
	for _, r := range records {
		if r.Address == address {
			return true
		}
	}
	return false
}

func recordID(records []rosRecord, address string) string {
	for _, record := range records {
		if record.Address == address {
			return record.ID
		}
	}
	return ""
}
