# Heliactyl Rust CLI

Command-line interface for managing Heliactyl server.

## Installation

Build the binary:
```bash
cargo build --release
```

The binary will be at `target/release/server-rs`

## Usage

### Start the Server

```bash
# Start with default config (./config.heli)
./server-rs serve

# Start with custom config
./server-rs serve --config /path/to/config.heli
```

### User Management

#### List All Users

```bash
./server-rs users list
```

Output:
```
ID                        Email                          Username                       Admin
-----------------------------------------------------------------------------------------------
507f1f77bcf86cd799439011  user@example.com               johndoe                        ✓ Yes
507f1f77bcf86cd799439012  admin@example.com              admin                          ✓ Yes
507f1f77bcf86cd799439013  test@example.com               testuser                       No

Total users: 3
```

#### Get User Details

```bash
# By email
./server-rs users get user@example.com

# By username
./server-rs users get johndoe
```

Output:
```
User Details:
  ID:       507f1f77bcf86cd799439011
  Email:    user@example.com
  Username: johndoe
  Admin:    Yes
  Created:  2024-01-15T10:30:00Z
  Updated:  2024-01-15T10:30:00Z
```

#### Set Admin Status

```bash
# Make user an admin (multiple formats accepted)
./server-rs users set-admin user@example.com true
./server-rs users set-admin user@example.com yes
./server-rs users set-admin user@example.com 1

# Remove admin status
./server-rs users set-admin johndoe false
./server-rs users set-admin johndoe no
./server-rs users set-admin johndoe 0
```

Accepted values for status: `true/false`, `yes/no`, `1/0` (case-insensitive)

Output:
```
✓ User 'johndoe' admin status set to: true
```

#### Delete User

```bash
# Delete with confirmation prompt
./server-rs users delete user@example.com

# Delete without confirmation
./server-rs users delete user@example.com --yes
```

### Custom Config Path

All commands support the `--config` flag:

```bash
./server-rs users list --config /path/to/config.heli
./server-rs users set-admin user@example.com true --config /path/to/config.heli
```

## Environment Variables

- `MONGODB` - Override MongoDB connection string from config

## Examples

### Make the first user an admin

```bash
# List users to find the email
./server-rs users list

# Set admin status
./server-rs users set-admin first-user@example.com true
```

### Batch operations

```bash
# List all users and save to file
./server-rs users list > users.txt

# Set multiple users as admin
for email in user1@example.com user2@example.com; do
  ./server-rs users set-admin "$email" true
done
```

### Quick admin check

```bash
# Check if a user is admin
./server-rs users get admin@example.com | grep "Admin:"
```

## Help

Get help for any command:

```bash
./server-rs --help
./server-rs users --help
./server-rs users list --help
./server-rs users set-admin --help
```
