package cmd

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/ourochronos/valence-cli/client"
	"github.com/ourochronos/valence-cli/config"
	"github.com/spf13/cobra"
)

var healthCmd = &cobra.Command{
	Use:   "health",
	Short: "Check API health",
	Long:  "Check if the Valence API is healthy and reachable.",
	Run: func(cmd *cobra.Command, args []string) {
		cfg := config.Load()
		c := client.NewClient(cfg.APIURL, cfg.AuthToken)

		resp, err := c.HealthCheck()
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}

		if cfg.Output == "json" {
			json.NewEncoder(os.Stdout).Encode(resp)
		} else {
			fmt.Println("API is healthy")
			if status, ok := resp["status"].(string); ok {
				fmt.Printf("  Status: %s\n", status)
			}
			if uptime, ok := resp["uptime"].(string); ok {
				fmt.Printf("  Uptime: %s\n", uptime)
			}
		}
	},
}

func init() {
	rootCmd.AddCommand(healthCmd)
}
