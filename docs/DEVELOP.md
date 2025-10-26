# Development Pattern

# Main server 
Endpoints
- /
The / endpoints to route all trafic to the client. Since helitactyl is a shit project. We need to make something high performance, DDOS protection and rate limiting out of the box 

## Plans for Rate limiting. 
Use redis and check if a client is sending too many requests or not. 


# Distributed Systems

Make it as simple as creating a file 
```json
{
    "name": "Fr-Helia",
    "Key" : "xxx_myheliaconnectkey",
    "nodes":{
            // Helia will automatically update this list and 
            // distribute trafic across the multiple
            // Backends
    }
}
Helia will automatically spread the load across multiple nodes.


Endpoints to finish

/api/me
/api/tenenats

