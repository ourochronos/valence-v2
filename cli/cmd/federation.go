package cmd

import (
	"fmt"

	"github.com/spf13/cobra"
)

var federationCmd = &cobra.Command{
	Use:   "federation",
	Short: "Federation operations",
	Long: `Federation operates via P2P libp2p networking (gossipsub + kademlia).
Federation HTTP API endpoints are not yet exposed — the federation manager
runs internally when the engine starts with --features federation.`,
}

var federationStatusCmd = &cobra.Command{
	Use:   "status",
	Short: "Get federation status",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Println("Federation status endpoint not yet exposed via HTTP API.")
		fmt.Println("Federation runs internally when the engine is built with --features federation.")
	},
}

var federationPeersCmd = &cobra.Command{
	Use:   "peers",
	Short: "List federation peers",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Println("Federation peers endpoint not yet exposed via HTTP API.")
		fmt.Println("Federation runs internally when the engine is built with --features federation.")
	},
}

func init() {
	rootCmd.AddCommand(federationCmd)
	federationCmd.AddCommand(federationStatusCmd)
	federationCmd.AddCommand(federationPeersCmd)
}
