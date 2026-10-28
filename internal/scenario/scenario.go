package scenario

import (
 "encoding/json"
 "errors"
 "fmt"
 "os"
 "strings"
)

type Node struct { ID string `json:"id"`; Latitude float64 `json:"latitude,omitempty"`; Longitude float64 `json:"longitude,omitempty"` }
type Scenario struct {
 Name string `json:"name"`
 Satellites []Node `json:"satellites"`
 Terminals []Node `json:"terminals"`
 Gateways []Node `json:"gateways"`
}
func Load(path string) (Scenario, error) {
 var s Scenario
 b, err := os.ReadFile(path); if err != nil { return s, err }
 // Bootstrap uses JSON (valid YAML 1.2 subset), despite .yaml extension.
 err = json.Unmarshal(b, &s)
 if err != nil { return s, fmt.Errorf("scenario must currently be JSON-compatible YAML: %w", err) }
 return s, nil
}
func (s Scenario) Validate() error {
 if strings.TrimSpace(s.Name)=="" { return errors.New("name required") }
 seen:=map[string]bool{}
 for _, group := range [][]Node{s.Satellites,s.Terminals,s.Gateways} {
  for _, n := range group {
   if n.ID=="" { return errors.New("node id required") }
   if seen[n.ID] { return fmt.Errorf("duplicate node id: %s",n.ID) };seen[n.ID]=true
   if n.Latitude < -90 || n.Latitude > 90 || n.Longitude < -180 || n.Longitude > 180 { return fmt.Errorf("invalid coordinates for %s",n.ID) }
  }
 }
 if len(s.Satellites)==0 || len(s.Terminals)==0 || len(s.Gateways)==0 { return errors.New("need satellite, terminal and gateway") }
 return nil
}
