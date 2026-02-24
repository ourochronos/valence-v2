package cmd

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/ourochronos/valence-cli/client"
	"github.com/ourochronos/valence-cli/config"
	"github.com/spf13/cobra"
)

var maintenanceCmd = &cobra.Command{
	Use:   "maintenance",
	Short: "Maintenance operations",
	Long:  "Run maintenance operations like decay, eviction, and embedding recomputation.",
}

var maintenanceDecayCmd = &cobra.Command{
	Use:   "decay",
	Short: "Trigger decay cycle",
	Long:  "Apply decay to all triples based on the decay factor.",
	Run: func(cmd *cobra.Command, args []string) {
		cfg := config.Load()
		c := client.NewClient(cfg.APIURL, cfg.AuthToken)

		factor, _ := cmd.Flags().GetFloat64("factor")
		minWeight, _ := cmd.Flags().GetFloat64("min-weight")

		req := client.DecayRequest{
			Factor:    factor,
			MinWeight: minWeight,
		}

		resp, err := c.TriggerDecay(req)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}

		if cfg.Output == "json" {
			json.NewEncoder(os.Stdout).Encode(resp)
		} else {
			fmt.Printf("Decay applied to %d triples\n", resp.AffectedCount)
		}
	},
}

var maintenanceEvictCmd = &cobra.Command{
	Use:   "evict",
	Short: "Evict low-weight triples",
	Long:  "Remove triples below a weight threshold.",
	Run: func(cmd *cobra.Command, args []string) {
		cfg := config.Load()
		c := client.NewClient(cfg.APIURL, cfg.AuthToken)

		threshold, _ := cmd.Flags().GetFloat64("threshold")

		req := client.EvictRequest{
			Threshold: threshold,
		}

		resp, err := c.TriggerEvict(req)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}

		if cfg.Output == "json" {
			json.NewEncoder(os.Stdout).Encode(resp)
		} else {
			fmt.Printf("Evicted %d triples\n", resp.EvictedCount)
		}
	},
}

var maintenanceRecomputeCmd = &cobra.Command{
	Use:   "recompute",
	Short: "Recompute embeddings",
	Long:  "Regenerate node embeddings from the current graph structure.",
	Run: func(cmd *cobra.Command, args []string) {
		cfg := config.Load()
		c := client.NewClient(cfg.APIURL, cfg.AuthToken)

		dimensions, _ := cmd.Flags().GetInt("dimensions")

		req := client.RecomputeEmbeddingsRequest{
			Dimensions: dimensions,
		}

		resp, err := c.RecomputeEmbeddings(req)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}

		if cfg.Output == "json" {
			json.NewEncoder(os.Stdout).Encode(resp)
		} else {
			fmt.Printf("Recomputed embeddings for %d nodes\n", resp.EmbeddingCount)
		}
	},
}

func init() {
	rootCmd.AddCommand(maintenanceCmd)
	maintenanceCmd.AddCommand(maintenanceDecayCmd)
	maintenanceCmd.AddCommand(maintenanceEvictCmd)
	maintenanceCmd.AddCommand(maintenanceRecomputeCmd)

	maintenanceDecayCmd.Flags().Float64("factor", 0.95, "Decay factor (0.0-1.0)")
	maintenanceDecayCmd.Flags().Float64("min-weight", 0.01, "Minimum weight threshold")

	maintenanceEvictCmd.Flags().Float64("threshold", 0.1, "Weight threshold for eviction")

	maintenanceRecomputeCmd.Flags().Int("dimensions", 64, "Embedding dimensions")
}
