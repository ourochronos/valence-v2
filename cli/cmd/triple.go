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

var tripleCmd = &cobra.Command{
	Use:   "triple",
	Short: "Manage triples",
	Long:  "Insert, query, and manage triples in the knowledge graph.",
}

var tripleInsertCmd = &cobra.Command{
	Use:   "insert",
	Short: "Insert a triple",
	Long:  "Insert a new triple into the knowledge graph.",
	Run: func(cmd *cobra.Command, args []string) {
		cfg := config.Load()
		c := client.NewClient(cfg.APIURL, cfg.AuthToken)

		subject, _ := cmd.Flags().GetString("subject")
		predicate, _ := cmd.Flags().GetString("predicate")
		object, _ := cmd.Flags().GetString("object")

		if subject == "" || predicate == "" || object == "" {
			fmt.Fprintln(os.Stderr, "Error: --subject, --predicate, and --object are required")
			os.Exit(1)
		}

		req := client.InsertTriplesRequest{
			Triples: []client.TripleInput{
				{
					Subject:   subject,
					Predicate: predicate,
					Object:    object,
				},
			},
		}

		resp, err := c.InsertTriples(req)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}

		if cfg.Output == "json" {
			json.NewEncoder(os.Stdout).Encode(resp)
		} else {
			fmt.Printf("Inserted triple: %s\n", resp.TripleIDs[0])
		}
	},
}

var tripleQueryCmd = &cobra.Command{
	Use:   "query",
	Short: "Query triples",
	Long:  "Query triples by subject, predicate, and/or object (wildcards supported).",
	Run: func(cmd *cobra.Command, args []string) {
		cfg := config.Load()
		c := client.NewClient(cfg.APIURL, cfg.AuthToken)

		subject, _ := cmd.Flags().GetString("subject")
		predicate, _ := cmd.Flags().GetString("predicate")
		object, _ := cmd.Flags().GetString("object")
		includeSources, _ := cmd.Flags().GetBool("include-sources")

		var subj, pred, obj *string
		if subject != "" {
			subj = &subject
		}
		if predicate != "" {
			pred = &predicate
		}
		if object != "" {
			obj = &object
		}

		resp, err := c.QueryTriples(subj, pred, obj, includeSources)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}

		if cfg.Output == "json" {
			json.NewEncoder(os.Stdout).Encode(resp)
		} else if cfg.Output == "table" {
			table := tablewriter.NewWriter(os.Stdout)
			table.SetHeader([]string{"ID", "Subject", "Predicate", "Object", "Weight"})
			for _, t := range resp.Triples {
				table.Append([]string{
					t.ID[:8],
					t.Subject.Value,
					t.Predicate,
					t.Object.Value,
					fmt.Sprintf("%.2f", t.Weight),
				})
			}
			table.Render()
		} else {
			for _, t := range resp.Triples {
				fmt.Printf("%s: %s -[%s]-> %s (weight: %.2f)\n",
					t.ID[:8], t.Subject.Value, t.Predicate, t.Object.Value, t.Weight)
			}
		}
	},
}

var tripleGetCmd = &cobra.Command{
	Use:   "get [triple-id]",
	Short: "Get sources for a triple",
	Long:  "Get provenance/source information for a specific triple.",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		cfg := config.Load()
		c := client.NewClient(cfg.APIURL, cfg.AuthToken)

		tripleID := args[0]

		resp, err := c.GetTripleSources(tripleID)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}

		if cfg.Output == "json" {
			json.NewEncoder(os.Stdout).Encode(resp)
		} else {
			fmt.Printf("Sources for triple %s:\n", tripleID[:8])
			for _, s := range resp.Sources {
				ref := ""
				if s.Reference != nil {
					ref = *s.Reference
				}
				fmt.Printf("  - %s: %s (ref: %s)\n", s.ID[:8], s.Type, ref)
			}
		}
	},
}

var tripleSearchCmd = &cobra.Command{
	Use:   "search",
	Short: "Search for similar nodes",
	Long:  "Perform semantic search to find nodes similar to a query.",
	Run: func(cmd *cobra.Command, args []string) {
		cfg := config.Load()
		c := client.NewClient(cfg.APIURL, cfg.AuthToken)

		query, _ := cmd.Flags().GetString("query")
		k, _ := cmd.Flags().GetInt("limit")

		if query == "" {
			fmt.Fprintln(os.Stderr, "Error: --query is required")
			os.Exit(1)
		}

		req := client.SearchRequest{
			QueryNode: query,
			K:         k,
		}

		resp, err := c.Search(req)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}

		if cfg.Output == "json" {
			json.NewEncoder(os.Stdout).Encode(resp)
		} else if cfg.Output == "table" {
			table := tablewriter.NewWriter(os.Stdout)
			table.SetHeader([]string{"Node ID", "Value", "Similarity"})
			for _, r := range resp.Results {
				table.Append([]string{
					r.NodeID[:8],
					r.Value,
					fmt.Sprintf("%.4f", r.Similarity),
				})
			}
			table.Render()
		} else {
			for _, r := range resp.Results {
				fmt.Printf("%s: %s (similarity: %.4f)\n", r.NodeID[:8], r.Value, r.Similarity)
			}
		}
	},
}

func init() {
	rootCmd.AddCommand(tripleCmd)
	tripleCmd.AddCommand(tripleInsertCmd)
	tripleCmd.AddCommand(tripleQueryCmd)
	tripleCmd.AddCommand(tripleGetCmd)
	tripleCmd.AddCommand(tripleSearchCmd)

	// Insert flags
	tripleInsertCmd.Flags().String("subject", "", "Subject node value")
	tripleInsertCmd.Flags().String("predicate", "", "Predicate value")
	tripleInsertCmd.Flags().String("object", "", "Object node value")

	// Query flags
	tripleQueryCmd.Flags().String("subject", "", "Filter by subject")
	tripleQueryCmd.Flags().String("predicate", "", "Filter by predicate")
	tripleQueryCmd.Flags().String("object", "", "Filter by object")
	tripleQueryCmd.Flags().Bool("include-sources", false, "Include source information")

	// Search flags
	tripleSearchCmd.Flags().String("query", "", "Query node value")
	tripleSearchCmd.Flags().Int("limit", 10, "Number of results to return")
}
