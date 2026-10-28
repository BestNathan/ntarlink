package scenario

import "testing"
func TestValidate(t *testing.T) {
 s:=Scenario{Name:"test",Satellites:[]Node{{ID:"sat-1"}},Terminals:[]Node{{ID:"ue-1"}},Gateways:[]Node{{ID:"gw-1"}}}
 if err:=s.Validate();err!=nil { t.Fatal(err) }
 s.Gateways[0].ID="sat-1"
 if err:=s.Validate();err==nil { t.Fatal("expected duplicate node error") }
}
