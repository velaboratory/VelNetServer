use std::io::prelude::*;
use std::thread;
use std::net::{TcpListener, TcpStream,UdpSocket,IpAddr,SocketAddr};
use std::collections::HashMap;
use std::sync::{Arc,RwLock};
use std::sync::mpsc;
use std::sync::mpsc::{SyncSender,Receiver};

enum ToClientTCPMessageType {
    LoggedIn = 0, 
    RoomList = 1,
    PlayerJoined = 2,
    DataMessage = 3,
    MasterMessage = 4,
    YouJoined = 5,
    PlayerLeft = 6,
    YouLeft = 7
}

enum FromClientTCPMessageType {
    LogIn = 0,
    GetRooms = 1,
    JoinRoom = 2,
    SendMessageOthersUnbuffered = 3,
    SendMessageAllUnbuffered = 4,
    SendMessageGroupUnbuffered = 5,
    CreateGroup = 6,
    SendMessageOthersBuffered = 7,
    SendMessageAllBuffered = 8
}

enum ToClientUDPMessageType {
    Connected = 0,
    DataMessage = ToClientTCPMessageType::DataMessage as isize
}
enum FromClientUDPMessageType {
    Connect = 0,
    SendMesssageOthersUnbuffered = FromClientTCPMessageType::SendMessageOthersUnbuffered as isize,
    SendMessageAllUnbuffered = FromClientTCPMessageType::SendMessageAllUnbuffered as isize,
    SendMessageGroupUnbuffered = FromClientTCPMessageType::SendMessageGroupUnbuffered as isize
}
struct Client {
    logged_in: Arc<RwLock<bool>>,
    id: u32,
    username: Arc<RwLock<String>>, 
    room: Arc<RwLock<Option<Arc<Room>>>>, 
    sender: SyncSender<Vec<u8>>,
    rooms_mutex: Arc<RwLock<HashMap<String,Arc<Room>>>>,
    clients_mutex: Arc<RwLock<HashMap<u32,Arc<Client>>>>,
    groups: Arc<RwLock<HashMap<String,Vec<Arc<Client>>>>>,
    ip: Arc<RwLock<IpAddr>>,
    port: Arc<RwLock<u16>>
}

struct Room {
    name: String,
    clients: RwLock<HashMap<u32,Arc<Client>>>,
    master_client: Arc<RwLock<Arc<Client>>>
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
    let mut string_bytes = vec![0;size as usize];
    stream.read_exact(&mut string_bytes).unwrap();
    return String::from_utf8(string_bytes).unwrap();
}
fn read_short_string(stream: &mut TcpStream) -> String {
    let size = read_u8(stream);
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

//this is in response to someone asking to login (this is where usernames and passwords would be processed, in theory)
fn read_login_message(stream: &mut TcpStream, client: &Arc<Client>) {
    //byte,shortstring,byte,shortstring
    let username = read_short_string(stream);
    let password = read_short_string(stream);

    println!("Got username {} and password {}",username,password);
    let mut client_user = client.username.write().unwrap();
    *client_user = username;
    let mut client_loggedin = client.logged_in.write().unwrap();
    *client_loggedin = true;
    let mut write_buf = vec![];
    write_buf.push(ToClientTCPMessageType::LoggedIn as u8);
    write_buf.extend_from_slice(&(client.id).to_be_bytes()); //send the client the id

    client.sender.send(write_buf).unwrap();
}

//this is in response to a request for rooms.
fn read_rooms_message(_stream: &mut TcpStream, client: &Arc<Client>){
    println!("Got rooms message");

    let mut write_buf = vec![];
    write_buf.push(ToClientTCPMessageType::RoomList as u8);
    //first we need to get the room names

    let rooms = client.rooms_mutex.read().unwrap();
    let mut rooms_vec = vec![];
    for (k,v) in rooms.iter() {
        let clients = v.clients.read().unwrap();
        let room_string = format!("{}:{}",k,clients.len());
        rooms_vec.push(room_string);
    }
    
    let rooms_message = rooms_vec.join(",");
    let message_bytes = rooms_message.as_bytes();
    let message_len = message_bytes.len() as u32;
    write_buf.extend_from_slice(&(message_len).to_be_bytes());
    write_buf.extend_from_slice(message_bytes);
    client.sender.send(write_buf).unwrap();
    
}


fn send_client_master_message(to: &Arc<Client>, master_id: u32){
  //2u8, person_id_u32, room_name_len_u8, room_name_bytes
  let mut write_buf = vec![];
  write_buf.push(ToClientTCPMessageType::MasterMessage as u8);
  write_buf.extend_from_slice(&(master_id).to_be_bytes()); //send everyone that the client id joined the room
  let res = to.sender.send(write_buf);
  match res {
      Ok(_) => (),
      Err(_) => ()
  }

}

fn send_client_join_message(to: &Arc<Client>, from: u32, room: &str){
    //2u8, person_id_u32, room_name_len_u8, room_name_bytes
    let mut write_buf = vec![];
    write_buf.push(ToClientTCPMessageType::PlayerJoined as u8);
    write_buf.extend_from_slice(&(from).to_be_bytes()); //send everyone that the client id joined the room
    write_buf.push(room.as_bytes().len() as u8); 
    write_buf.extend_from_slice(room.as_bytes());
    let res = to.sender.send(write_buf);
    match res {
        Ok(_) => (),
        Err(_) => ()
    }
}

fn send_you_joined_message(to: &Arc<Client>, in_room: Vec<u32>, room: &str){
    //you_joined_u8, ids_len_u32, id_list_array_u32, room_name_len_u8, room_name_bytes
    let mut write_buf = vec![];
    write_buf.push(ToClientTCPMessageType::YouJoined as u8);
    write_buf.extend_from_slice(&(in_room.len() as u32).to_be_bytes());  
    for id in in_room {
        write_buf.extend_from_slice(&(id).to_be_bytes());
    }
    write_buf.push(room.as_bytes().len() as u8); 
    write_buf.extend_from_slice(room.as_bytes());
    let res = to.sender.send(write_buf);
    match res {
        Ok(_) => (),
        Err(_) => ()
    }
}

fn send_you_left_message(to: &Arc<Client>, room: &str){
    let mut write_buf = vec![];
    write_buf.push(ToClientTCPMessageType::YouLeft as u8);
    write_buf.push(room.as_bytes().len() as u8); 
    write_buf.extend_from_slice(room.as_bytes());
    let res = to.sender.send(write_buf);
    match res {
        Ok(_) => (),
        Err(_) => ()
    }
}
fn send_client_left_message(to: &Arc<Client>, from: u32, room: &str){
    let mut write_buf = vec![];
    write_buf.push(ToClientTCPMessageType::PlayerLeft as u8);
    write_buf.extend_from_slice(&(from).to_be_bytes()); //send everyone that the client id left the room
    write_buf.push(room.as_bytes().len() as u8); 
    write_buf.extend_from_slice(room.as_bytes());
    let res = to.sender.send(write_buf);
    match res {
        Ok(_) => (),
        Err(_) => ()
    }
}


//helper function, because clients leave room in multiple places
fn client_leave_room(client: &Arc<Client>, send_to_client: bool){
    //first remove the client from the room they are in
    
    let mut room = client.room.write().unwrap();
    
    if room.is_some(){
        {
            println!("Client in room, leaving");
            let room = room.as_ref().unwrap();

            //may have to choose a new master
            let mut master_client = room.master_client.write().unwrap();
            let mut change_master = false;
            let mut clients = room.clients.write().unwrap();

            let mut new_master_id = 0;

            if master_client.id == client.id {
                println!("Will change master");
                //change the master
                change_master = true;
                for (_k,v) in clients.iter() {
                    if v.id != client.id {
                        new_master_id = v.id;
                        break;
                    }
                }
            }



            println!("Client leaving current room {}",&room.name);
                       
            
            for (_k,v) in clients.iter() {
                if !send_to_client && v.id == client.id{
                    continue;
                }else if v.id == client.id {
                    send_you_left_message(v, &room.name);
                }else{
                    send_client_left_message(v, client.id, &room.name); 
                }
            }
            clients.remove(&client.id); //remove the client from that list in the room

            //if the room is empty, destroy it as well
            
            if clients.len() == 0 {
                let mut rooms = client.rooms_mutex.write().unwrap(); 
                rooms.remove(&room.name);
                println!("Destroyed room {}",&room.name)
            }else if change_master{
                println!("Changing master to {}",new_master_id);
                for (_k,v) in clients.iter() {
                    send_client_master_message(&v, new_master_id);
                }

                
                *master_client = clients.get(&new_master_id).unwrap().clone();
                
            }
        }
    } else{
        println!("Client not in room");
    }
    
    *room = Option::None;
    

}

fn read_join_message(stream: &mut TcpStream, client: &Arc<Client>){
    //byte,shortstring
    let mut room_name = read_short_string(stream);
    room_name = room_name.trim().to_string();
    println!("Got room message {}",room_name);

   
    //if the client is in a room, leave it
    let mut leave_room = false;
    {
        let room = client.room.read().unwrap();  //must release this mutex before calling into a function that uses it
        if room.as_ref().is_some(){
            println!("Leaving the current room ");
            leave_room = true;
        }
    }
    if leave_room {
        client_leave_room(client, true); 
    }

    if room_name.trim() == "" || room_name == "-1" {
        println!("Empty room, leaving");
        return;
    }
    
    
    
    
    //join room_name
    {
        let mut rooms = client.rooms_mutex.write().unwrap(); 
        if !rooms.contains_key(&room_name) { //new room, must create it
            let map: HashMap<u32, Arc<Client>> = HashMap::new();
            let r = Arc::new(Room {
                name: room_name.to_string(),
                clients: RwLock::new(map),
                master_client: Arc::new(RwLock::new(client.clone())) //client is the master, since they joined first
            });
            rooms.insert(String::from(&room_name),r);
            println!("New room {} created",&room_name);
        }
        
        //the room is guaranteed to exist now
        {
            let mut clients = rooms[&room_name].clients.write().unwrap(); 
            clients.insert(client.id,client.clone());
            println!("Client {} joined {}",client.id,&room_name);
            {
                let mut room = client.room.write().unwrap(); 
                *room = Some(rooms.get(&room_name).unwrap().clone()); //we create an option and assign it back to the room
            }
            
            //send a join message to everyone in the room (except the client)
            for (_k,v) in clients.iter() {
                if v.id != client.id {
                    send_client_join_message(v, client.id, &room_name);
                }
            }

            //send a join message to the client that has all of the ids in the room
            let mut ids_in_room = vec![];  
            for (_k,v) in clients.iter() {
                ids_in_room.push(v.id);
                
            }
            
            send_you_joined_message(client, ids_in_room, &room_name);


            let room = client.room.read().unwrap(); 
            //tell the client who the master is
            let master_client = room.as_ref().unwrap().master_client.read().unwrap();
            send_client_master_message(client, master_client.id);
        }
    }
    
}

// function send_message_to_clients_dictionary(clients: message: &Vec<u8>, include_sender: bool){
    
        
       
        
// }

fn send_room_message(sender: &Arc<Client>, message: &Vec<u8>, include_sender: bool, ordered: bool){
    //this message is 3u8, sender_id_u32, message_len_u32, message_bytes
    let mut write_buf = vec![];
    write_buf.push(ToClientTCPMessageType::DataMessage as u8);
    write_buf.extend_from_slice(&sender.id.to_be_bytes());
    write_buf.extend_from_slice(&(message.len() as u32).to_be_bytes());
    write_buf.extend_from_slice(message);
    //println!("sending {} bytes from {}",message.len(),sender.id);
    {
        
        if !ordered {
            let room = sender.room.read().unwrap(); 
            
            if room.is_some() {

                let clients = room.as_ref().unwrap().clients.read().unwrap();
                for (_k,v) in clients.iter(){
                    if !include_sender && v.id == sender.id {
                        continue;
                    }
                    match v.sender.send(write_buf.clone()){
                        Ok(_) => (),
                        Err(x) => println!("Error sending to client {}: {}",v.id,x)
                    } //this sometimes fails. 

                }
            }
        }else{ //I'm bad at rust, so I don't know how else to do this other than repeat the code above because the types are so different
            let room = sender.room.write().unwrap(); 
            
            if room.is_some() {

                let clients = room.as_ref().unwrap().clients.read().unwrap();
                for (_k,v) in clients.iter(){
                    if !include_sender && v.id == sender.id {
                        continue;
                    }
                    match v.sender.send(write_buf.clone()){
                        Ok(_) => (),
                        Err(x) => println!("Error sending to client {}: {}",v.id,x)
                    } //this sometimes fails. 

                }
            }
        }
    }
}
fn send_group_message(sender: &Arc<Client>, message: &Vec<u8>, group: &String){
    let mut write_buf = vec![];
    write_buf.push(ToClientTCPMessageType::DataMessage as u8);
    write_buf.extend_from_slice(&sender.id.to_be_bytes());
    write_buf.extend_from_slice(&(message.len() as u32).to_be_bytes());
    write_buf.extend_from_slice(message);

    //get the list of client ids for this group
    let groups = sender.groups.read().unwrap();
    if groups.contains_key(group) {
      let group = groups.get(group).unwrap();
      for c in group {
          //there may be a leftover when a client leaves...will fix itself

          match c.sender.send(write_buf.clone()) {
              Ok(_) => (),
              Err(_) => println!("no client in group")
          }
      }
    }

}
fn read_send_message(stream: &mut TcpStream, client: &Arc<Client>, message_type: u8){
    //4 byte length, array
    //this is a message for everyone in the room (maybe)
    let to_send = read_vec(stream);
    if message_type == FromClientTCPMessageType::SendMessageOthersUnbuffered as u8 {
       send_room_message(client,&to_send,false,false);
    }else if message_type == FromClientTCPMessageType::SendMessageAllUnbuffered as u8 {
       send_room_message(client,&to_send,true,false);
    } else if message_type == FromClientTCPMessageType::SendMessageOthersBuffered as u8 { //ordered
        send_room_message(client,&to_send,false,true);
     }else if message_type == FromClientTCPMessageType::SendMessageAllBuffered as u8 { //ordered
        send_room_message(client,&to_send,true,true);
     }else if message_type == FromClientTCPMessageType::SendMessageGroupUnbuffered as u8 {
       let group = read_short_string(stream);
       send_group_message(client,&to_send, &group);
    }
}

fn read_group_message(stream: &mut TcpStream, client: &Arc<Client>){
    let mut groups = client.groups.write().unwrap();
    let group = read_short_string(stream);
    let id_bytes = read_vec(stream);
    let num = id_bytes.len();
    let clients = client.clients_mutex.read().unwrap();
    let mut group_clients = vec![];
    let mut i = 0;
    loop {
        if i >= num {
            break;
        }
        let mut slice = [0u8;4];
        slice[0] = id_bytes[i]; 
        slice[1] = id_bytes[i+1]; 
        slice[2] = id_bytes[i+2]; 
        slice[3] = id_bytes[i+3]; //probably a better way to do this
        let id = u32::from_be_bytes(slice);
        

        match clients.get(&id) {
            Some(client) => {group_clients.push(client.clone());},
            None => () //not there, so don't add it
        }
        
        i = i + 4;
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

        //println!("Got a message {}",read_buf[0]);
        let t = read_buf[0];
        if t == FromClientTCPMessageType::LogIn as u8 { //[0:u8][username.length():u8][username:shortstring][password.length():u8][password:shortstring]
            read_login_message(&mut stream, &mut client);
        } else if t == FromClientTCPMessageType::GetRooms as u8 {//[1:u8]
            read_rooms_message(&mut stream, &mut client);
        } else if t == FromClientTCPMessageType::JoinRoom as u8 {//[2:u8][roomname.length():u8][roomname:shortstring]
            read_join_message(&mut stream, &mut client); 
        } else if t == FromClientTCPMessageType::SendMessageOthersUnbuffered as u8 || 
                  t == FromClientTCPMessageType::SendMessageAllUnbuffered as u8 || 
                  t == FromClientTCPMessageType::SendMessageGroupUnbuffered as u8 || 
                  t == FromClientTCPMessageType::SendMessageOthersBuffered as u8 || 
                  t == FromClientTCPMessageType::SendMessageAllBuffered as u8 { //others,all,group[t:u8][message.length():i32][message:u8array]
            read_send_message(&mut stream, &client, t);
        } else if t == FromClientTCPMessageType::CreateGroup as u8 { //[t:u8][list.lengthbytes:i32][clients:i32array]
            read_group_message(&mut stream, &client);
        }
            
        std::io::stdout().flush().unwrap();
    }
}

fn client_write_thread(mut stream: TcpStream, rx: Receiver<Vec<u8>> ) {

    //wait on messages in my queue
    loop {
        let m = rx.recv().unwrap();
        //println!("Sending a message {}",m.len());
        if m.len() == 1{
            break;
        }
        stream.write(&m).unwrap();

    }
}


fn handle_client(stream: TcpStream, client_id: u32, clients_mutex: Arc<RwLock<HashMap<u32,Arc<Client>>>>, rooms_mutex: Arc<RwLock<HashMap<String,Arc<Room>>>>){
    
    stream.set_nodelay(true).unwrap();
    println!("Accepted new connection");
    
    let (tx, rx) = mpsc::sync_channel(10000);
    //create a new client structure and add it to the list of clients
    let client = Arc::new(Client{
        id: client_id,
        username: Arc::new(RwLock::new(String::from(""))),
        logged_in: Arc::new(RwLock::new(false)),
        room: Arc::new(RwLock::new(Option::None)),
        sender: tx,
        rooms_mutex: rooms_mutex.clone(),
        clients_mutex: clients_mutex.clone(),
        groups: Arc::new(RwLock::new(HashMap::new())),
        ip: Arc::new(RwLock::new(stream.peer_addr().unwrap().ip())),
        port: Arc::new(RwLock::new(0)) 
    });

    {
        let mut clients = clients_mutex.write().unwrap();
        clients.insert(client_id, client.clone());
    }
    
    
    let read_clone = stream.try_clone().expect("clone failed");
    let read_client = client.clone();
    let read_handle = thread::spawn(move ||{client_read_thread(read_clone, read_client)});
    let write_handle = thread::spawn(move ||{client_write_thread(stream, rx)});

    //handle writing to the thread as needed

    match read_handle.join(){
        Ok(_)=>(),
        Err(_)=>()
    }
    
    match client.sender.send(vec![0]){
        Ok(_)=>(),
        Err(_)=>()
    }
    
    match write_handle.join() {
        Ok(_)=>(),
        Err(_)=>()
    }
    println!("Client {} left",client_id);
    //now we can kill the client.  
    {
        //make sure we remove the client from all rooms
        client_leave_room(&client, false);
        let mut clients = clients_mutex.write().unwrap();
        clients.remove(&client_id);
    }
    
}

fn tcp_listen(client_mutex: Arc<RwLock<HashMap<u32, Arc<Client>>>>, room_mutex: Arc<RwLock<HashMap<String,Arc<Room>>>>){
    println!("Started TCP Listener");
    let listener = TcpListener::bind("0.0.0.0:80").expect("could not bind port");

    let mut next_client_id = 0;
    // accept connections and process them serially
    for stream in listener.incoming() {
        let client_mutex = Arc::clone(&client_mutex);
        let room_mutex = Arc::clone(&room_mutex);
        thread::spawn(move || {handle_client(stream.unwrap(), next_client_id, client_mutex, room_mutex)});
        next_client_id+=1;
    }
}

fn udp_listen(client_mutex: Arc<RwLock<HashMap<u32, Arc<Client>>>>, _room_mutex: Arc<RwLock<HashMap<String,Arc<Room>>>>){
    let mut buf = [0u8;1024];
    let s = UdpSocket::bind("0.0.0.0:80").unwrap();
    println!("UDP Thread Started");
    loop {
        let res = s.recv_from(&mut buf);
        match res {
            Ok(_) => (),
            Err(_) => continue
        }
        let (packet_size,addr) = res.unwrap();
        let t = buf[0];
        if packet_size >= 5{
            //get the client id, which has to be sent with every udp message, because you don't know where udp messages are coming from
            let client_id_bytes = [buf[1],buf[2],buf[3],buf[4]];
            let client_id = u32::from_be_bytes(client_id_bytes);

            

            if t == FromClientUDPMessageType::Connect as u8 { //1 byte, 0.  Nothing else.  This is just to establish the udp port, Echos back the same thing sent
                //connect message, respond back
                {
                    let clients = client_mutex.read().unwrap();
                    if clients.contains_key(&client_id){
                        let client = clients.get(&client_id).unwrap();
                        let mut port = client.port.write().unwrap();
                        *port = addr.port(); //set the udp port to send data to
                        buf[0] = ToClientUDPMessageType::Connected as u8;
                        match s.send_to(&buf,addr) {
                            Ok(_)=>(),
                            Err(_)=>()
                        }
                    }
                }
                
                
            } else if t == FromClientUDPMessageType::SendMesssageOthersUnbuffered as u8 { //[3:u8][from:i32][contents:u8array] note that it must fit into the packet of 1024 bytes
                {
                    let clients = client_mutex.read().unwrap();
                    if clients.contains_key(&client_id){
                        let client = clients.get(&client_id).unwrap();
                        let room_option = client.room.read().unwrap();
                        let room = room_option.as_ref().unwrap();
                        let room_clients = room.clients.read().unwrap(); //we finally got to the room!
                        buf[0] = ToClientUDPMessageType::DataMessage as u8; //technically unecessary, unless we change this number
                        for (_k,v) in room_clients.iter() {
                            if v.id != client_id{
                                let ip = v.ip.read().unwrap();
                                let port = v.port.read().unwrap();
                                match s.send_to(&buf,SocketAddr::new(*ip, *port)) {
                                    Ok(_)=> (),
                                    Err(_) => ()
                                }
                            }
                        }
                    }
                }

            } else if t == FromClientUDPMessageType::SendMessageAllUnbuffered as u8 { //see above
                {
                    let clients = client_mutex.read().unwrap();
                    if clients.contains_key(&client_id){
                        let client = clients.get(&client_id).unwrap();
                        let room_option = client.room.read().unwrap();
                        let room = room_option.as_ref().unwrap();
                        let room_clients = room.clients.read().unwrap(); //we finally got to the room!
                        buf[0] = ToClientUDPMessageType::DataMessage as u8; //messages are always 3s, even though this came in as 4 
                        for (_k,v) in room_clients.iter() {
                            
                            let ip = v.ip.read().unwrap();
                            let port = v.port.read().unwrap();
                            match s.send_to(&buf,SocketAddr::new(*ip, *port)) {
                                Ok(_)=> (),
                                Err(_) => ()
                            }
                            
                        }
                    }
                }
            } else if t == FromClientUDPMessageType::SendMessageGroupUnbuffered as u8 { //[5:byte][from:i32][group.length():u8][message:u8array]
                //this one is a little different, because we don't send the group in the message, so we have to formulate another message (like a 3 message)
                //send a message to a group
                //read the group name
                
                let group_name_size = buf[5];
                let message_vec = buf[6..packet_size].to_vec();
                let (group_name_bytes, message_bytes) = message_vec.split_at(group_name_size as usize);
                let group_name = String::from_utf8(group_name_bytes.to_vec()).unwrap();
                let clients = client_mutex.read().unwrap();
                if clients.contains_key(&client_id){
                    let client = clients.get(&client_id).unwrap();
                    let groups = client.groups.read().unwrap();
                    if groups.contains_key(&group_name) {
                        
                    
                        let clients = groups.get(&group_name).unwrap();
                        
                        

                        //we need to form a new message without the group name 
                        let mut message_to_send = vec![];
                        message_to_send.push(ToClientUDPMessageType::DataMessage as u8);
                        message_to_send.extend([buf[1],buf[2],buf[3],buf[4]]);
                        message_to_send.extend(message_bytes);
                        
                        for v in clients.iter() {
                            
                            let ip = v.ip.read().unwrap();
                            let port = v.port.read().unwrap();
                            
                            match s.send_to(&message_to_send,SocketAddr::new(*ip, *port)) {
                                Ok(_)=> (),
                                Err(_) => ()
                            }
                        }
                    }
                }
            }

            
        }
    
    }
    

    
}

fn main() {
    println!("VelNet Server Starting");

    let clients: HashMap<u32, Arc<Client>> = HashMap::new();
    let rooms: HashMap<String, Arc<Room>> = HashMap::new();
    let client_mutex = Arc::new(RwLock::new(clients));
    let room_mutex = Arc::new(RwLock::new(rooms));

    //start the UDP thread
    let udp_clients = Arc::clone(&client_mutex);
    let udp_rooms = Arc::clone(&room_mutex);
    let udp_handle = thread::spawn(move ||{udp_listen(udp_clients, udp_rooms);});
    //start the TCP thread
    tcp_listen(client_mutex, room_mutex);
    udp_handle.join().unwrap();
    println!("VelNet Ended");
}
