package cmd

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/ourochronos/valence-cli/client"
	"github.com/ourochronos/valence-cli/config"
	"github.com/spf13/cobra"
)

var trustDID string

var trustCmd = &cobra.Command{
	Use:   "trust",
	Short: "Trust and reputation queries",
	Long:  "Query trust scores derived from PageRank of DID nodes in the knowledge graph.",
}

var trustCheckCmd = &cobra.Command{
	Use:   "check --did <DID>",
	Short: "Check trust level for a DID",
	Run: func(cmd *cobra.Command, args []string) {
		if trustDID == "" {
			fmt.Fprintln(os.Stderr, "Error: --did flag is required")
			os.Exit(1)
		}

		cfg := config.Load()
		c := client.NewClient(cfg.APIURL, cfg.AuthToken)

		resp, err := c.TrustQuery(trustDID)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}

		if cfg.Output == "json" {
			json.NewEncoder(os.Stdout).Encode(resp)
		} else {
			fmt.Printf("DID:         %s\n", resp.DID)
			fmt.Printf("Trust Score: %.4f\n", resp.TrustScore)
			if len(resp.ConnectedDIDs) > 0 {
				fmt.Println("Connected DIDs:")
				for _, e := range resp.ConnectedDIDs {
					fmt.Printf("  %-50s  %.4f\n", e.DID, e.TrustScore)
				}
			}
		}
	},
}

var reputationGetCmd = &cobra.Command{
	Use:   "reputation [identity]",
	Short: "Get reputation score (alias for trust check)",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		cfg := config.Load()
		c := client.NewClient(cfg.APIURL, cfg.AuthToken)

		resp, err := c.TrustQuery(args[0])
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}

		if cfg.Output == "json" {
			json.NewEncoder(os.Stdout).Encode(resp)
		} else {
			fmt.Printf("DID:         %s\n", resp.DID)
			fmt.Printf("Trust Score: %.4f\n", resp.TrustScore)
		}
	},
}

func init() {
	rootCmd.AddCommand(trustCmd)
	trustCmd.AddCommand(trustCheckCmd)
	trustCmd.AddCommand(reputationGetCmd)

	trustCheckCmd.Flags().StringVar(&trustDID, "did", "", "DID to check trust for")
}
