package cmd

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/ourochronos/valence-cli/client"
	"github.com/ourochronos/valence-cli/config"
	"github.com/spf13/cobra"
)

var statsCmd = &cobra.Command{
	Use:   "stats",
	Short: "Get engine statistics",
	Long:  "Display statistics about the knowledge graph.",
	Run: func(cmd *cobra.Command, args []string) {
		cfg := config.Load()
		c := client.NewClient(cfg.APIURL, cfg.AuthToken)

		resp, err := c.GetStats()
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}

		if cfg.Output == "json" {
			json.NewEncoder(os.Stdout).Encode(resp)
		} else {
			fmt.Println("Engine Statistics:")
			fmt.Printf("  Triples: %d\n", resp.TripleCount)
			fmt.Printf("  Nodes:   %d\n", resp.NodeCount)
			fmt.Printf("  Average Weight: %.4f\n", resp.AvgWeight)
		}
	},
}

func init() {
	rootCmd.AddCommand(statsCmd)
}
