use std::io::prelude::*;
use std::thread;
use std::net::{TcpListener, TcpStream};
use std::collections::HashMap;
use std::sync::{Arc,Mutex};
use std::io;
use std::sync::mpsc;
use std::sync::mpsc::{SyncSender,Receiver};
struct Client {
    logged_in: bool,
    id: u32,
    name: String,
    room: Option<Room>,
    sender: SyncSender<Vec<u8>>,
    rooms_mutex: Arc<Mutex<HashMap<String,Room>>>
}

struct Room {
    clients: Mutex<HashMap<u32,Arc<Client>>>
}

fn udp_listen(){
    println!("UDP Thread Started");
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

fn read_login_message(stream: &mut TcpStream, client: Arc<Client>) {
    println!("Got login message");
    let username = read_string(stream);
    let password = read_string(stream);

    println!("Got username {} and password {}",username,password);
    
    let mut writeBuf = vec![];
    writeBuf.push(0u8);
    writeBuf.extend_from_slice(&(client.id).to_be_bytes()); //send the client the id

    client.sender.send(writeBuf);
}

fn read_rooms_message(stream: &mut TcpStream, client: Arc<Client>){

}

fn read_join_message(stream: &mut TcpStream, client: Arc<Client>){
    let room_name = read_string(stream);

    println!("Got room message {}",room_name);

    //if the client is in a room, leave it

    if !client.room.is_none(){
        //we must leave our current room
        //todo
    }

    
    //join that room
    {
        let mut rooms = client.rooms_mutex.lock().unwrap();
        if !rooms.contains_key(&room_name) {
            let map: HashMap<u32, Arc<Client>> = HashMap::new();
            let r = Room {
                clients: Mutex::new(map)
            };
            rooms.insert(String::from(&room_name),r);
            println!("New room {} created",&room_name);


        }
        
        {
            let mut clients = rooms[&room_name].clients.lock().unwrap();
            clients.insert(client.id,client.clone());
            println!("Client {} joined {}",client.id,&room_name);

            //send a join message to everyone in the room
            for (k,v) in clients.iter() {
                let mut writeBuf = vec![];
                writeBuf.push(2u8);
                writeBuf.extend_from_slice(&(client.id).to_be_bytes()); //send everyone that the client id joined the room
                v.sender.send(writeBuf);
            }

            //send a join message to the client for everyone else in the room (so they get a join message)
            for (k,v) in clients.iter() {
                if v.id != client.id {
                    let mut writeBuf = vec![];
                    writeBuf.push(2u8);
                    writeBuf.extend_from_slice(&(v.id).to_be_bytes()); //send everyone that the client id joined the room
                    client.sender.send(writeBuf);
                }
            }
        }
    }
    
}


fn client_read_thread(mut stream: TcpStream, client: Arc<Client>) {
    let mut readBuf:[u8;1] = [0; 1];
    //messages come through as a 4-bit type identifier, that can be one of 0 (login) 1 (get rooms), 2 (join/leave room) 3(send message others) 4(send message all) 5(send message group)
    loop {

        //read exactly 1 byte
        stream.read_exact(&mut readBuf);

        println!("Got a message {}",readBuf[0]);
        let t = readBuf[0];
        if t == 0 {
            read_login_message(&mut stream, client.clone());
        } else if t == 1 {
            read_rooms_message(&mut stream, client.clone());
        } else if t == 2 {
            read_join_message(&mut stream, client.clone()); 
        }
            
        
    }
}

fn client_write_thread(mut stream: TcpStream, client: Arc<Client>, rx: Receiver<Vec<u8>> ) {

    //wait on messages in my queue
    loop {
        let m = rx.recv().unwrap();
        //write the login message out
        stream.write(&m).unwrap();
    }
}


fn handle_client(mut stream: TcpStream, client_id: u32, clients_mutex: Arc<Mutex<HashMap<u32,Arc<Client>>>>, rooms_mutex: Arc<Mutex<HashMap<String,Room>>>){
    
    stream.set_nodelay(true).unwrap();
    println!("Accepted new connection");
    
    let (tx, rx) = mpsc::sync_channel(10000);
    //create a new client structure and add it to the list of clients
    let client = Arc::new(Client{
        id: client_id,
        name: String::from(""),
        logged_in: false,
        room: Option::None,
        sender: tx,
        rooms_mutex: rooms_mutex
    });

    {
        let mut clients = clients_mutex.lock().unwrap();
        clients.insert(client_id, client.clone());
    }
    
    
    let read_clone = stream.try_clone().expect("clone failed");
    let read_client = client.clone();
    let write_client = client.clone();
    let read_handle = thread::spawn(move ||{client_read_thread(read_clone, read_client)});
    let write_handle = thread::spawn(move ||{client_write_thread(stream, write_client, rx)});

    //handle writing to the thread as needed
    println!("Writing process started");



    let res = read_handle.join();
    client.sender.send(vec![0]).unwrap();
    println!("Client left");
    let res = write_handle.join();
    
}

fn tcp_listen(){
    println!("Started TCP Listener");
    let listener = TcpListener::bind("127.0.0.1:80").expect("could not bind port");
    
    let clients: HashMap<u32, Arc<Client>> = HashMap::new();
    let rooms: HashMap<String, Room> = HashMap::new();
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
