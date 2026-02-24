package cmd

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/ourochronos/valence-cli/client"
	"github.com/ourochronos/valence-cli/config"
	"github.com/spf13/cobra"
)

var sessionPlatform string
var sessionProject string

var sessionCmd = &cobra.Command{
	Use:   "session",
	Short: "Session management",
	Long:  "Manage VKB sessions for tracking conversations and context.",
}

var sessionStartCmd = &cobra.Command{
	Use:   "start",
	Short: "Start a session",
	Run: func(cmd *cobra.Command, args []string) {
		cfg := config.Load()
		c := client.NewClient(cfg.APIURL, cfg.AuthToken)

		req := client.SessionStartRequest{
			Platform: sessionPlatform,
		}
		if sessionProject != "" {
			req.ProjectContext = &sessionProject
		}

		resp, err := c.SessionStart(req)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}

		if cfg.Output == "json" {
			json.NewEncoder(os.Stdout).Encode(resp)
		} else {
			fmt.Printf("Session started: %s\n", resp.ID)
			fmt.Printf("  Status: %s\n", resp.Status)
		}
	},
}

var sessionEndCmd = &cobra.Command{
	Use:   "end [session-id]",
	Short: "End a session",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		cfg := config.Load()
		c := client.NewClient(cfg.APIURL, cfg.AuthToken)

		req := client.SessionEndRequest{}
		err := c.SessionEnd(args[0], req)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}

		fmt.Printf("Session %s ended\n", args[0])
	},
}

var sessionListCmd = &cobra.Command{
	Use:   "list",
	Short: "List sessions",
	Run: func(cmd *cobra.Command, args []string) {
		cfg := config.Load()
		c := client.NewClient(cfg.APIURL, cfg.AuthToken)

		sessions, err := c.SessionList(nil, 20)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}

		if cfg.Output == "json" {
			json.NewEncoder(os.Stdout).Encode(sessions)
		} else {
			if len(sessions) == 0 {
				fmt.Println("No sessions found")
				return
			}
			for _, s := range sessions {
				fmt.Printf("%-36s  %-12s  %-10s  %s\n", s.ID, s.Platform, s.Status, s.CreatedAt)
			}
		}
	},
}

var sessionGetCmd = &cobra.Command{
	Use:   "get [session-id]",
	Short: "Get session details",
	Args:  cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		cfg := config.Load()
		c := client.NewClient(cfg.APIURL, cfg.AuthToken)

		resp, err := c.SessionGet(args[0])
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}

		if cfg.Output == "json" {
			json.NewEncoder(os.Stdout).Encode(resp)
		} else {
			fmt.Printf("Session: %s\n", resp.ID)
			fmt.Printf("  Platform: %s\n", resp.Platform)
			fmt.Printf("  Status:   %s\n", resp.Status)
			fmt.Printf("  Created:  %s\n", resp.CreatedAt)
			if resp.ProjectContext != nil {
				fmt.Printf("  Project:  %s\n", *resp.ProjectContext)
			}
			if resp.Summary != nil {
				fmt.Printf("  Summary:  %s\n", *resp.Summary)
			}
			if len(resp.Themes) > 0 {
				fmt.Printf("  Themes:   %v\n", resp.Themes)
			}
		}
	},
}

func init() {
	rootCmd.AddCommand(sessionCmd)
	sessionCmd.AddCommand(sessionStartCmd)
	sessionCmd.AddCommand(sessionEndCmd)
	sessionCmd.AddCommand(sessionListCmd)
	sessionCmd.AddCommand(sessionGetCmd)

	sessionStartCmd.Flags().StringVar(&sessionPlatform, "platform", "api", "Platform (claude-code, api, slack, etc.)")
	sessionStartCmd.Flags().StringVar(&sessionProject, "project", "", "Project context")
}
