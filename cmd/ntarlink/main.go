package main

import (
 "flag"
 "fmt"
 "os"
 "strings"

 "github.com/BestNathan/ntarlink/internal/scenario"
)

func main() {
 if len(os.Args) < 2 { usage(); os.Exit(2) }
 switch os.Args[1] {
 case "validate", "simulate":
  fs := flag.NewFlagSet(os.Args[1], flag.ExitOnError)
  path := fs.String("scenario", "scenarios/minimal.yaml", "scenario file")
  _ = fs.Parse(os.Args[2:])
  s, err := scenario.Load(*path)
  if err != nil { fmt.Fprintln(os.Stderr, "scenario:", err); os.Exit(1) }
  if err := s.Validate(); err != nil { fmt.Fprintln(os.Stderr, "invalid scenario:", err); os.Exit(1) }
  fmt.Printf("scenario=%s satellites=%d terminals=%d gateways=%d\\n", s.Name, len(s.Satellites), len(s.Terminals), len(s.Gateways))
  if os.Args[1] == "simulate" { fmt.Println("simulation engine: not yet implemented (bootstrap only)") }
 default:
  fmt.Fprintln(os.Stderr, "unknown command:", strings.Join(os.Args[1:], " ")); usage(); os.Exit(2)
 }
}
func usage() { fmt.Fprintln(os.Stderr, "usage: ntarlink <validate|simulate> [-scenario path]") }
