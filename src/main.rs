use std::io::prelude::*;
use std::thread;
use std::net::{TcpListener, TcpStream};
use std::collections::HashMap;
use std::sync::{Arc,Mutex};
use std::io;
use std::sync::mpsc;
use std::sync::mpsc::{SyncSender,Receiver};
struct Client {
    logged_in: Arc<Mutex<bool>>,
    id: u32,
    username: Arc<Mutex<String>>, 
    room: Arc<Mutex<Option<Arc<Room>>>>, 
    sender: SyncSender<Vec<u8>>,
    rooms_mutex: Arc<Mutex<HashMap<String,Arc<Room>>>>
}

struct Room {
    name: String,
    clients: Mutex<HashMap<u32,Arc<Client>>>
}

fn udp_listen(){
    println!("UDP Thread Started");
}

fn read_u8(stream: &mut TcpStream) -> u8 {
    let mut buf = [0; 1];
    stream.read_exact(&mut buf).unwrap();
    return buf[0];
}
fn read_u32(stream: &mut TcpStream) -> u32 {
    let mut buf:[u8;4] = [0; 4];
    stream.read_exact(&mut buf).unwrap();
    let mut size = u32::from_be_bytes(buf);
    return size;
}
fn read_string(stream: &mut TcpStream) -> String {
    let size = read_u32(stream);
    println!("Size in bytes: {}",size);
    let mut stringBytes = vec![0;size as usize];
    stream.read_exact(&mut stringBytes).unwrap();
    return String::from_utf8(stringBytes).unwrap();
}
fn read_short_string(stream: &mut TcpStream) -> String {
    let size = read_u8(stream);
    println!("Size in bytes: {}",size);
    let mut stringBytes = vec![0;size as usize];
    stream.read_exact(&mut stringBytes).unwrap();
    return String::from_utf8(stringBytes).unwrap();
}

fn read_vec(stream: &mut TcpStream) -> Vec<u8> {
    let message_size = read_u32(stream);
    let mut message = vec![0u8;message_size as usize];
    stream.read_exact(&mut message).unwrap();
    return message;
}

fn read_login_message(stream: &mut TcpStream, mut client: &Arc<Client>) {
    println!("Got login message");
    let username = read_short_string(stream);
    let password = read_short_string(stream);

    println!("Got username {} and password {}",username,password);
    let mut client_user = client.username.lock().unwrap();
    *client_user = username;
    let mut client_loggedin = client.logged_in.lock().unwrap();
    *client_loggedin = true;
    let mut writeBuf = vec![];
    writeBuf.push(0u8);
    writeBuf.extend_from_slice(&(client.id).to_be_bytes()); //send the client the id

    client.sender.send(writeBuf);
}

fn read_rooms_message(stream: &mut TcpStream, mut client: &Arc<Client>){

}

fn send_client_join_message(to: &Arc<Client>, from: u32, room: &str){
    //this message is 2u8, person_id_u32, room_name_len_u8, room_name_bytes
    let mut writeBuf = vec![];
    writeBuf.push(2u8);
    writeBuf.extend_from_slice(&(from).to_be_bytes()); //send everyone that the client id joined the room
    writeBuf.push(room.as_bytes().len() as u8); 
    writeBuf.extend_from_slice(room.as_bytes());
    to.sender.send(writeBuf).unwrap();
}

fn client_leave_room(mut client: &Arc<Client>, send_to_client: bool){
    //first remove the client from the room they are in
    
    let mut room = client.room.lock().unwrap();
    
    if room.is_some(){
        {
            let room = room.as_ref().unwrap();
            println!("Client leaving current room {}",&room.name);
            let mut clients = room.clients.lock().unwrap();           
            for (k,v) in clients.iter() {
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

fn read_join_message(stream: &mut TcpStream, mut client: &Arc<Client>){
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
            *room = Some(rooms[&room_name].clone());
            //send a join message to everyone in the room
            for (k,v) in clients.iter() {
                send_client_join_message(v, client.id, &room_name);
            }

            //send a join message to the client for everyone else in the room (so they get a join message)
            for (k,v) in clients.iter() {
                if v.id != client.id {
                    send_client_join_message(&client, v.id, &room_name);
                }
            }
        }
    }
    
}

fn send_room_message(sender: &Arc<Client>, message: &Vec<u8>, include_sender: bool){
    //this message is 3u8, sender_id_u32, message_len_u32, message_bytes
    let mut writeBuf = vec![];
    writeBuf.push(3u8);
    writeBuf.extend_from_slice(&sender.id.to_be_bytes());
    writeBuf.extend_from_slice(&(message.len() as u32).to_be_bytes());
    writeBuf.extend_from_slice(message);
    println!("sending {} bytes from {}",message.len(),sender.id);
    {
        let room = sender.room.lock().unwrap();
        if room.is_some() {

            let clients = room.as_ref().unwrap().clients.lock().unwrap();
            for (k,v) in clients.iter(){
                if !include_sender && v.id == sender.id {
                    continue;
                }
                v.sender.send(writeBuf.clone()).unwrap();
            }
        }
    }
    //lock the clients in the room as well, because someone might leave the room in the middle...though, I suppose they'd have to lock the room to do it?
}

fn read_send_message(stream: &mut TcpStream, client: &Arc<Client>, message_type: u8){
    //this is a message for everyone in the room (maybe)
    let to_send = read_vec(stream);
    if message_type == 3 {
       send_room_message(client,&to_send,false);
    }else if message_type == 4 {
        //everyone in my group
    }else if message_type == 5 {
        //everyone including me
    }
}

fn client_read_thread(mut stream: TcpStream, mut client: Arc<Client>) {
    let mut readBuf:[u8;1] = [0; 1];
    //messages come through as a 4-bit type identifier, that can be one of 0 (login) 1 (get rooms), 2 (join/leave room) 3(send message others) 4(send message all) 5(send message group)
    loop {

        //read exactly 1 byte
        stream.read_exact(&mut readBuf);

        println!("Got a message {}",readBuf[0]);
        let t = readBuf[0];
        if t == 0 {
            read_login_message(&mut stream, &mut client);
        } else if t == 1 {
            read_rooms_message(&mut stream, &mut client);
        } else if t == 2 {
            read_join_message(&mut stream, &mut client); 
        } else if t == 3 || t == 4 {
            read_send_message(&mut stream, &client, t);
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


fn handle_client(mut stream: TcpStream, client_id: u32, clients_mutex: Arc<Mutex<HashMap<u32,Arc<Client>>>>, rooms_mutex: Arc<Mutex<HashMap<String,Arc<Room>>>>){
    
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
        rooms_mutex: rooms_mutex
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

    let res = read_handle.join();
    client.sender.send(vec![0]).unwrap();
    
    let res = write_handle.join();
    println!("Client left");
    //now we can kill the client.  
    {
        //make sure we remove the client from all rooms
        client_leave_room(&client, false);
        let mut clients = clients_mutex.lock().unwrap();
        clients.remove(&client_id);
    }
    
}

fn tcp_listen(){
    println!("Started TCP Listener");
    let listener = TcpListener::bind("127.0.0.1:80").expect("could not bind port");
    
    let clients: HashMap<u32, Arc<Client>> = HashMap::new();
    let rooms: HashMap<String, Arc<Room>> = HashMap::new();
    let client_mutex = Arc::new(Mutex::new(clients));
    let room_mutex = Arc::new(Mutex::new(rooms));
    let mut next_client_id = 0;
    // accept connections and process them serially
    for stream in listener.incoming() {
        let client_mutex = Arc::clone(&client_mutex);
        let room_mutex = Arc::clone(&room_mutex);
        thread::spawn(move || {handle_client(stream.unwrap(), next_client_id, client_mutex, room_mutex)});
        next_client_id+=1;
    }
}

fn main() {
    println!("VelNet Server Starting");

    //start the UDP thread
    let udp_handle = thread::spawn(udp_listen);
    //start the TCP thread
    tcp_listen();
    udp_handle.join().unwrap();
    println!("VelNet Ended");
}
