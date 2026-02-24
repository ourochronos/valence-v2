package cmd

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/olekukonko/tablewriter"
	"github.com/ourochronos/valence-cli/client"
	"github.com/ourochronos/valence-cli/config"
	"github.com/spf13/cobra"
)

var nodeCmd = &cobra.Command{
	Use:   "node",
	Short: "Node operations",
	Long:  "Explore node neighborhoods and relationships.",
}

var nodeNeighborsCmd = &cobra.Command{
	Use:   "neighbors [node]",
	Short: "Get k-hop neighbors",
	Long:  "Get the k-hop neighborhood of a node (can be node ID or value).",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		cfg := config.Load()
		c := client.NewClient(cfg.APIURL, cfg.AuthToken)

		node := args[0]
		depth, _ := cmd.Flags().GetInt("depth")

		resp, err := c.GetNeighbors(node, depth)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}

		if cfg.Output == "json" {
			json.NewEncoder(os.Stdout).Encode(resp)
		} else if cfg.Output == "table" {
			fmt.Printf("Found %d triples involving %d nodes\n\n", resp.TripleCount, resp.NodeCount)
			table := tablewriter.NewWriter(os.Stdout)
			table.SetHeader([]string{"Subject", "Predicate", "Object", "Weight"})
			for _, t := range resp.Triples {
				table.Append([]string{
					t.Subject.Value,
					t.Predicate,
					t.Object.Value,
					fmt.Sprintf("%.2f", t.Weight),
				})
			}
			table.Render()
		} else {
			fmt.Printf("Found %d triples involving %d nodes\n", resp.TripleCount, resp.NodeCount)
			for _, t := range resp.Triples {
				fmt.Printf("%s -[%s]-> %s (weight: %.2f)\n",
					t.Subject.Value, t.Predicate, t.Object.Value, t.Weight)
			}
		}
	},
}

func init() {
	rootCmd.AddCommand(nodeCmd)
	nodeCmd.AddCommand(nodeNeighborsCmd)

	nodeNeighborsCmd.Flags().Int("depth", 1, "Depth of neighborhood to explore")
}
