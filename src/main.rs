use std::io::prelude::*;
use std::thread;
use std::net::{TcpListener, TcpStream,UdpSocket,IpAddr};
use std::collections::HashMap;
use std::sync::{Arc,Mutex};
use std::sync::mpsc;
use std::sync::mpsc::{SyncSender,Receiver};
struct Client {
    logged_in: Arc<Mutex<bool>>,
    id: u32,
    username: Arc<Mutex<String>>, 
    room: Arc<Mutex<Option<Arc<Room>>>>, 
    sender: SyncSender<Vec<u8>>,
    rooms_mutex: Arc<Mutex<HashMap<String,Arc<Room>>>>,
    clients_mutex: Arc<Mutex<HashMap<u32,Arc<Client>>>>,
    groups: Arc<Mutex<HashMap<String,Vec<Arc<Client>>>>>,
    ip: Arc<Mutex<IpAddr>>,
    port: Arc<Mutex<u16>>
}

struct Room {
    name: String,
    clients: Mutex<HashMap<u32,Arc<Client>>>
}



fn read_u8(stream: &mut TcpStream) -> u8 {
    let mut buf = [0; 1];
    stream.read_exact(&mut buf).unwrap();
    return buf[0];
}
fn read_u32(stream: &mut TcpStream) -> u32 {
    let mut buf:[u8;4] = [0; 4];
    stream.read_exact(&mut buf).unwrap();
    let size = u32::from_be_bytes(buf);
    return size;
}
fn _read_string(stream: &mut TcpStream) -> String {
    let size = read_u32(stream);
    println!("Size in bytes: {}",size);
    let mut string_bytes = vec![0;size as usize];
    stream.read_exact(&mut string_bytes).unwrap();
    return String::from_utf8(string_bytes).unwrap();
}
fn read_short_string(stream: &mut TcpStream) -> String {
    let size = read_u8(stream);
    println!("Size in bytes: {}",size);
    let mut string_bytes = vec![0;size as usize];
    stream.read_exact(&mut string_bytes).unwrap();
    return String::from_utf8(string_bytes).unwrap();
}

fn read_vec(stream: &mut TcpStream) -> Vec<u8> {
    let message_size = read_u32(stream);
    let mut message = vec![0u8;message_size as usize];
    stream.read_exact(&mut message).unwrap();
    return message;
}

fn read_login_message(stream: &mut TcpStream, client: &Arc<Client>) {
    //byte,shortstring,byte,shortstring
    println!("Got login message");
    let username = read_short_string(stream);
    let password = read_short_string(stream);

    println!("Got username {} and password {}",username,password);
    let mut client_user = client.username.lock().unwrap();
    *client_user = username;
    let mut client_loggedin = client.logged_in.lock().unwrap();
    *client_loggedin = true;
    let mut write_buf = vec![];
    write_buf.push(0u8);
    write_buf.extend_from_slice(&(client.id).to_be_bytes()); //send the client the id

    client.sender.send(write_buf).unwrap();
}

fn read_rooms_message(_stream: &mut TcpStream, mut _client: &Arc<Client>){

}

fn send_client_join_message(to: &Arc<Client>, from: u32, room: &str){
    
    //2u8, person_id_u32, room_name_len_u8, room_name_bytes
    let mut write_buf = vec![];
    write_buf.push(2u8);
    write_buf.extend_from_slice(&(from).to_be_bytes()); //send everyone that the client id joined the room
    write_buf.push(room.as_bytes().len() as u8); 
    write_buf.extend_from_slice(room.as_bytes());
    let res = to.sender.send(write_buf);
    match res {
        Ok(_) => println!("send successful"),
        Err(_) => println!("send unsuccessful")
    }
}

fn client_leave_room(client: &Arc<Client>, send_to_client: bool){
    //first remove the client from the room they are in
    
    let mut room = client.room.lock().unwrap();
    
    if room.is_some(){
        {
            let room = room.as_ref().unwrap();
            println!("Client leaving current room {}",&room.name);
            let mut clients = room.clients.lock().unwrap();           
            for (_k,v) in clients.iter() {
                if !send_to_client && v.id == client.id{
                    continue;
                }
                send_client_join_message(v, client.id, "") //send the leave room message to everyone in the room
            }
            clients.remove(&client.id); //remove the client from that list in the room

            //if the room is empty, destroy it as well
            
            if clients.len() == 0 {
                let mut rooms = client.rooms_mutex.lock().unwrap(); 
                rooms.remove(&room.name);
                println!("Destroyed room {}",&room.name)
            }
        }
    }
    *room = Option::None;
    

}

fn read_join_message(stream: &mut TcpStream, client: &Arc<Client>){
    //byte,shortstring
    let room_name = read_short_string(stream);

    println!("Got room message {}",room_name);

    //if the client is in a room, leave it

    let mut room = client.room.lock().unwrap(); 
    if room.as_ref().is_some(){
        client_leave_room(client, true); 
    }
    
    //join room_name
    {
        let mut rooms = client.rooms_mutex.lock().unwrap(); 
        if !rooms.contains_key(&room_name) { //new room, must create it
            let map: HashMap<u32, Arc<Client>> = HashMap::new();
            let r = Arc::new(Room {
                name: room_name.to_string(),
                clients: Mutex::new(map)
            });
            rooms.insert(String::from(&room_name),r);
            println!("New room {} created",&room_name);
        }
        
        //the room is guaranteed to exist now
        {
            let mut clients = rooms[&room_name].clients.lock().unwrap(); 
            clients.insert(client.id,client.clone());
            println!("Client {} joined {}",client.id,&room_name);
            *room = Some(rooms.get(&room_name).unwrap().clone()); //we create an option and assign it back to the room
            //send a join message to everyone in the room
            for (_k,v) in clients.iter() {
                send_client_join_message(v, client.id, &room_name);
            }

            //send a join message to the client for everyone else in the room (so they get a join message)
            for (_k,v) in clients.iter() {
                if v.id != client.id {
                    send_client_join_message(&client, v.id, &room_name);
                }
            }
        }
    }
    
}

fn send_room_message(sender: &Arc<Client>, message: &Vec<u8>, include_sender: bool){
    //this message is 3u8, sender_id_u32, message_len_u32, message_bytes
    let mut write_buf = vec![];
    write_buf.push(3u8);
    write_buf.extend_from_slice(&sender.id.to_be_bytes());
    write_buf.extend_from_slice(&(message.len() as u32).to_be_bytes());
    write_buf.extend_from_slice(message);
    println!("sending {} bytes from {}",message.len(),sender.id);
    {
        let room = sender.room.lock().unwrap();
        if room.is_some() {

            let clients = room.as_ref().unwrap().clients.lock().unwrap();
            for (_k,v) in clients.iter(){
                if !include_sender && v.id == sender.id {
                    continue;
                }
                v.sender.send(write_buf.clone()).unwrap();
            }
        }
    }
}
fn send_group_message(sender: &Arc<Client>, message: &Vec<u8>, group: &String){

    let mut write_buf = vec![];
    write_buf.push(3u8);
    write_buf.extend_from_slice(&sender.id.to_be_bytes());
    write_buf.extend_from_slice(&(message.len() as u32).to_be_bytes());
    write_buf.extend_from_slice(message);

    //get the list of client ids for this group
    let groups = sender.groups.lock().unwrap();
    let group = groups.get(group).unwrap();
    for c in group {
        c.sender.send(write_buf.clone()).unwrap();
    }

}
fn read_send_message(stream: &mut TcpStream, client: &Arc<Client>, message_type: u8){
    //4 byte length, array
    //this is a message for everyone in the room (maybe)
    let to_send = read_vec(stream);
    if message_type == 3 {
       send_room_message(client,&to_send,false);
    }else if message_type == 4 {
       send_room_message(client,&to_send,true);
    }else if message_type == 5 {
        let group = read_short_string(stream);
       send_group_message(client,&to_send, &group);
    }
}

fn read_group_message(stream: &mut TcpStream, client: &Arc<Client>){
    let mut groups = client.groups.lock().unwrap();
    let group = read_short_string(stream);
    let id_bytes = read_vec(stream);
    let num = id_bytes.len();
    let clients = client.clients_mutex.lock().unwrap();
    let mut group_clients = vec![];
    for i in 0..num {
        let mut slice = [0u8;4];
        slice[0] = id_bytes[i]; 
        slice[1] = id_bytes[i+1]; 
        slice[2] = id_bytes[i+2]; 
        slice[3] = id_bytes[i+3]; //probably a better way to do this
        let id = u32::from_be_bytes(slice);
        

        let client = clients.get(&id).unwrap();
        group_clients.push(client.clone());
    }

    //delete the group if it exists
    if groups.contains_key(&group) {
        groups.remove(&group); //ensures the client references go away
    }
    
    groups.insert(group.clone(),group_clients); 

}

fn client_read_thread(mut stream: TcpStream, mut client: Arc<Client>) {
    let mut read_buf:[u8;1] = [0; 1];
    //messages come through as a 1 byte type identifier, that can be one of 0 (login) 1 (get rooms), 2 (join/leave room) 3 (send message to room), 4 (send message to room including me), 5 (send message to group), 6 (establish group)
    loop {

        //read exactly 1 byte
        stream.read_exact(&mut read_buf).unwrap();

        println!("Got a message {}",read_buf[0]);
        let t = read_buf[0];
        if t == 0 {
            read_login_message(&mut stream, &mut client);
        } else if t == 1 {
            read_rooms_message(&mut stream, &mut client);
        } else if t == 2 {
            read_join_message(&mut stream, &mut client); 
        } else if t == 3 || t == 4 || t==5 {
            read_send_message(&mut stream, &client, t);
        } else if t == 6 {
            read_group_message(&mut stream, &client);
        }
            
        
    }
}

fn client_write_thread(mut stream: TcpStream, rx: Receiver<Vec<u8>> ) {

    //wait on messages in my queue
    loop {
        let m = rx.recv().unwrap();
        println!("Sending a message {}",m.len());
        if m.len() == 1{
            break;
        }
        stream.write(&m).unwrap();

    }
}


fn handle_client(stream: TcpStream, client_id: u32, clients_mutex: Arc<Mutex<HashMap<u32,Arc<Client>>>>, rooms_mutex: Arc<Mutex<HashMap<String,Arc<Room>>>>){
    
    stream.set_nodelay(true).unwrap();
    println!("Accepted new connection");
    
    let (tx, rx) = mpsc::sync_channel(10000);
    //create a new client structure and add it to the list of clients
    let client = Arc::new(Client{
        id: client_id,
        username: Arc::new(Mutex::new(String::from(""))),
        logged_in: Arc::new(Mutex::new(false)),
        room: Arc::new(Mutex::new(Option::None)),
        sender: tx,
        rooms_mutex: rooms_mutex.clone(),
        clients_mutex: clients_mutex.clone(),
        groups: Arc::new(Mutex::new(HashMap::new())),
        ip: Arc::new(Mutex::new(stream.peer_addr().unwrap().ip())),
        port: Arc::new(Mutex::new(0)) 
    });

    {
        let mut clients = clients_mutex.lock().unwrap();
        clients.insert(client_id, client.clone());
    }
    
    
    let read_clone = stream.try_clone().expect("clone failed");
    let read_client = client.clone();
    let read_handle = thread::spawn(move ||{client_read_thread(read_clone, read_client)});
    let write_handle = thread::spawn(move ||{client_write_thread(stream, rx)});

    //handle writing to the thread as needed
    println!("Writing process started");

    match read_handle.join(){
        Ok(_)=>(),
        Err(_)=>()
    }
    
    client.sender.send(vec![0]).unwrap();
    
    match write_handle.join() {
        Ok(_)=>(),
        Err(_)=>()
    }
    println!("Client left");
    //now we can kill the client.  
    {
        //make sure we remove the client from all rooms
        client_leave_room(&client, false);
        let mut clients = clients_mutex.lock().unwrap();
        clients.remove(&client_id);
    }
    
}

fn tcp_listen(client_mutex: Arc<Mutex<HashMap<u32, Arc<Client>>>>, room_mutex: Arc<Mutex<HashMap<String,Arc<Room>>>>){
    println!("Started TCP Listener");
    let listener = TcpListener::bind("127.0.0.1:80").expect("could not bind port");

    let mut next_client_id = 0;
    // accept connections and process them serially
    for stream in listener.incoming() {
        let client_mutex = Arc::clone(&client_mutex);
        let room_mutex = Arc::clone(&room_mutex);
        thread::spawn(move || {handle_client(stream.unwrap(), next_client_id, client_mutex, room_mutex)});
        next_client_id+=1;
    }
}

fn udp_listen(client_mutex: Arc<Mutex<HashMap<u32, Arc<Client>>>>, _room_mutex: Arc<Mutex<HashMap<String,Arc<Room>>>>){
    let mut buf = [0u8;1024];
    let s = UdpSocket::bind("127.0.0.1:80").unwrap();
    println!("UDP Thread Started");
    loop {
        let (packet_size,addr) = s.recv_from(&mut buf).unwrap();
        println!("Got a UDP packet of size {}",packet_size);
        let t = buf[0];
        if packet_size > 5{
            //get the client id, which has to be sent with every udp message, because you don't know where udp messages are coming from
            let client_id_bytes = [buf[1],buf[2],buf[3],buf[4]];
            let client_id = u32::from_be_bytes(client_id_bytes);

            if t == 0 {
                //connect message, respond back
                {
                    let clients = client_mutex.lock().unwrap();
                    let client = clients.get(&client_id).unwrap();
                    let mut port = client.port.lock().unwrap();
                    *port = addr.port(); //set the udp port to send data to
                    s.send_to(&buf,addr).unwrap(); //echo back

                }
                
                
            } else if t == 3 {
                

            }

            
        }
    
    }
    

    
}

fn main() {
    println!("VelNet Server Starting");

    let clients: HashMap<u32, Arc<Client>> = HashMap::new();
    let rooms: HashMap<String, Arc<Room>> = HashMap::new();
    let client_mutex = Arc::new(Mutex::new(clients));
    let room_mutex = Arc::new(Mutex::new(rooms));

    //start the UDP thread
    let udp_clients = Arc::clone(&client_mutex);
    let udp_rooms = Arc::clone(&room_mutex);
    let udp_handle = thread::spawn(move ||{udp_listen(udp_clients, udp_rooms);});
    //start the TCP thread
    tcp_listen(client_mutex, room_mutex);
    udp_handle.join().unwrap();
    println!("VelNet Ended");
}
