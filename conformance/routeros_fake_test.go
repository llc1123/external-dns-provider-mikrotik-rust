package conformance

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"sync"
	"testing"
)

type rosRecord struct {
	ID             string `json:".id"`
	Name           string `json:"name"`
	Type           string `json:"type"`
	Address        string `json:"address,omitempty"`
	Text           string `json:"text"`
	TTL            string `json:"ttl,omitempty"`
	Comment        string `json:"comment,omitempty"`
	Disabled       string `json:"disabled,omitempty"`
	Regexp         string `json:"regexp,omitempty"`
	MatchSubdomain string `json:"match-subdomain,omitempty"`
	AddressList    string `json:"address-list,omitempty"`
	CNAME          string `json:"cname,omitempty"`
	MXExchange     string `json:"mx-exchange,omitempty"`
	MXPreference   string `json:"mx-preference,omitempty"`
	SrvPort        string `json:"srv-port,omitempty"`
	SrvTarget      string `json:"srv-target,omitempty"`
	SrvPriority    string `json:"srv-priority,omitempty"`
	SrvWeight      string `json:"srv-weight,omitempty"`
	NS             string `json:"ns,omitempty"`
}

type routerOSFake struct {
	mu      sync.Mutex
	records []rosRecord
	nextID  int
	writes  int
	failGet bool
	server  *httptest.Server
}

func newRouterOSFake(t *testing.T) *routerOSFake {
	t.Helper()
	fake := &routerOSFake{nextID: 1, records: make([]rosRecord, 0)}
	fake.server = httptest.NewServer(http.HandlerFunc(fake.handle))
	t.Cleanup(fake.server.Close)
	return fake
}

func (f *routerOSFake) handle(w http.ResponseWriter, r *http.Request) {
	const collection = "/rest/ip/dns/static"
	if r.URL.Path != collection && !strings.HasPrefix(r.URL.Path, collection+"/") {
		http.NotFound(w, r)
		return
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	switch r.Method {
	case http.MethodGet:
		if f.failGet {
			f.failGet = false
			http.Error(w, "injected RouterOS failure", http.StatusInternalServerError)
			return
		}
		_ = json.NewEncoder(w).Encode(f.records)
	case http.MethodPut, http.MethodPatch:
		if (r.Method == http.MethodPut) != (r.URL.Path == collection) {
			http.Error(w, "wrong RouterOS method", http.StatusMethodNotAllowed)
			return
		}
		var record rosRecord
		if err := json.NewDecoder(r.Body).Decode(&record); err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}
		if r.URL.Path == collection {
			record.ID = "*" + strconv.Itoa(f.nextID)
			f.nextID++
			f.records = append(f.records, record)
			f.writes++
			w.WriteHeader(http.StatusCreated)
		} else {
			id := strings.TrimPrefix(r.URL.Path, collection+"/")
			found := false
			for i := range f.records {
				if f.records[i].ID == id {
					record.ID = id
					f.records[i] = record
					f.writes++
					found = true
					break
				}
			}
			if !found {
				http.NotFound(w, r)
				return
			}
		}
		_ = json.NewEncoder(w).Encode(record)
	case http.MethodDelete:
		id := r.URL.Query().Get(".id")
		if id == "" {
			id = strings.TrimPrefix(r.URL.Path, collection+"/")
		}
		for i, record := range f.records {
			if record.ID == id {
				f.records = append(f.records[:i], f.records[i+1:]...)
				f.writes++
				w.WriteHeader(http.StatusNoContent)
				return
			}
		}
		http.NotFound(w, r)
	default:
		w.WriteHeader(http.StatusMethodNotAllowed)
	}
}

func (f *routerOSFake) snapshot() []rosRecord {
	f.mu.Lock()
	defer f.mu.Unlock()
	return append([]rosRecord(nil), f.records...)
}

func (f *routerOSFake) writeCount() int {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.writes
}

func (f *routerOSFake) injectReadError() {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.failGet = true
}

func (f *routerOSFake) addForeign(record rosRecord) {
	f.mu.Lock()
	defer f.mu.Unlock()
	record.ID = "*" + strconv.Itoa(f.nextID)
	f.nextID++
	f.records = append(f.records, record)
}

func (f *routerOSFake) drift(address, ttl string) {
	f.mu.Lock()
	defer f.mu.Unlock()
	for i := range f.records {
		if f.records[i].Address == address {
			f.records[i].TTL = ttl
			return
		}
	}
}
