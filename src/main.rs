use async_std::prelude::*;
use async_std::net::TcpListener;
use async_std::net::TcpStream;
use async_std::net::UdpSocket;
use async_notify::Notify;
use futures::stream::StreamExt;
use futures::future;
use futures::join;
use futures::select;
use futures::pin_mut;
use futures::future::FutureExt;
use async_std::net::{IpAddr,SocketAddr};
use std::collections::HashMap;
use std::rc::{Rc};
use std::cell::{RefCell};
use std::fs;
use chrono::Local;
use std::time;
use serde::{Serialize, Deserialize};
use std::error::Error;
enum ToClientTCPMessageType {
    LoggedIn = 0, 
    RoomList = 1,
    PlayerJoined = 2,
    DataMessage = 3,
    MasterMessage = 4,
    YouJoined = 5,
    PlayerLeft = 6,
    YouLeft = 7,
    RoomData = 8
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
    SendMessageAllBuffered = 8,
    GetRoomData =9
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
    logged_in: bool,
    id: u32,
    username: String, 
    roomname: String,
    application: String,
    groups: HashMap<String,Vec<Rc<RefCell<Client>>>>,
    ip: IpAddr,
    udp_port: u16,
    message_queue: Vec<u8>,
    message_queue_udp: Vec<Vec<u8>>,
    notify: Rc<Notify>,
    notify_udp: Rc<Notify>,
    is_master: bool
}

struct Room {
    name: String,
    clients: HashMap<u32,Rc<RefCell<Client>>>,
    master_client: Rc<RefCell<Client>>
}
#[derive(Serialize, Deserialize)]
struct Config {
    port: u16,
    tcp_timeout: u64,
    tcp_send_buffer: usize
}


#[async_std::main]
async fn main() {
    
    println!("{}: VelNet Server Starting",Local::now().format("%Y-%m-%d %H:%M:%S"));
    
    //read the config file
    let foo = fs::read_to_string("config.txt").unwrap();
    let config: Config = serde_json::from_str(&foo).unwrap();
    println!("{}",config.port);
    
    let tcp_listener = TcpListener::bind(format!("0.0.0.0:{}",config.port)).await.unwrap();
    let udp_socket = Rc::new(RefCell::new(UdpSocket::bind(format!("0.0.0.0:{}",config.port)).await.unwrap()));

    let clients: Rc<RefCell<HashMap<u32, Rc<RefCell<Client>>>>> = Rc::new(RefCell::new(HashMap::new()));
    let rooms: Rc<RefCell<HashMap<String, Rc<RefCell<Room>>>>> = Rc::new(RefCell::new(HashMap::new()));
    let last_client_id = Rc::new(RefCell::new(0));
    
    let tcp_future = tcp_listener
        .incoming()
        .for_each_concurrent(None, |tcpstream| process_client(tcpstream.unwrap(), udp_socket.clone(), clients.clone(),rooms.clone(),last_client_id.clone()));
    let udp_future = process_udp(udp_socket.clone(),clients.clone(),rooms.clone());
    join!(tcp_future,udp_future);

}

async fn process_client(socket: TcpStream, udp_socket: Rc<RefCell<UdpSocket>>, clients: Rc<RefCell<HashMap<u32, Rc<RefCell<Client>>>>>, rooms: Rc<RefCell<HashMap<String, Rc<RefCell<Room>>>>>, last_client_id: Rc<RefCell<u32>>){
    
    println!("started tcp");
    
    let my_id;
    {
        let mut reference = last_client_id.borrow_mut();
        *reference = *reference + 1;
        my_id = *reference;
    }

     
    let client_notify = Rc::new(Notify::new());
    let client_notify_udp = Rc::new(Notify::new());
    let client = Rc::new(RefCell::new(Client{
        id: my_id,
        username: String::from(""),
        logged_in: false,
        roomname: String::from(""),
        application: String::from(""),
        groups: HashMap::new(),
        ip: socket.peer_addr().unwrap().ip(),
        udp_port: 0 as u16,
        message_queue: vec![],
        message_queue_udp: vec![],
        notify: client_notify.clone(),
        notify_udp: client_notify_udp.clone(),
        is_master: false
    }));


    println!("Spawned client handler = {}",my_id);
    
    {
        let temp_client = client.clone();
        let mut clients_temp = clients.borrow_mut();
        clients_temp.insert(my_id,temp_client);
    }



    let read_async = client_read(client.clone(), socket.clone(), clients.clone(), rooms.clone()).fuse();
    let write_async = client_write(client.clone(), socket, client_notify.clone()).fuse();
    let write_async_udp = client_write_udp(client.clone(), udp_socket.clone(), client_notify_udp.clone()).fuse();
   
    pin_mut!(read_async,write_async,write_async_udp); //not sure why this is necessary, since select 

    select! {  //
        () = read_async => println!("read async ended"),
        () = write_async => println!("write async ended"),
        () = write_async_udp => println!("write async udp ended")
    }
    
    {
        client_leave_room(client.clone(), false, rooms.clone());
        let mut clients_temp = clients.borrow_mut();
        clients_temp.remove(&client.borrow().id);
    }
    {
        println!("Client {} left",client.borrow().id);
    }

}

async fn client_read(client: Rc<RefCell<Client>>, mut socket: TcpStream, clients: Rc<RefCell<HashMap<u32, Rc<RefCell<Client>>>>>, rooms: Rc<RefCell<HashMap<String, Rc<RefCell<Room>>>>>){
   
    let mut buf = [0; 1];

    loop {
        match socket.read(&mut buf).await {
            // socket closed
            Ok(n) if n == 0 => {
                println!("client read ended naturally?");
                return;
            },
            Ok(_) => {
                
                let t = buf[0];
                if t == FromClientTCPMessageType::LogIn as u8 { //[0:u8][username.length():u8][username:shortstring][password.length():u8][password:shortstring]
                    match read_login_message(socket.clone(), client.clone()).await{ 
                        Ok(_)=>(),
                        Err(_)=>{eprintln!("failed to read from socket"); return;}
                    };
                } else if t == FromClientTCPMessageType::GetRooms as u8 {//[1:u8]
                    match read_rooms_message(socket.clone(), client.clone(), rooms.clone()).await{ 
                        Ok(_)=>(),
                        Err(_)=>{eprintln!("failed to read from socket"); return;}
                    };
                } else if t == FromClientTCPMessageType::GetRoomData as u8 {
                    match read_roomdata_message(socket.clone(), client.clone(), rooms.clone()).await{ 
                        Ok(_)=>(),
                        Err(_)=>{eprintln!("failed to read from socket"); return;}
                    };
                } else if t == FromClientTCPMessageType::JoinRoom as u8 {//[2:u8][roomname.length():u8][roomname:shortstring]
                    match read_join_message(socket.clone(), client.clone(), rooms.clone()).await{ 
                        Ok(_)=>(),
                        Err(_)=>{eprintln!("failed to read from socket"); return;}
                    }; 
                } else if t == FromClientTCPMessageType::SendMessageOthersUnbuffered as u8 || 
                          t == FromClientTCPMessageType::SendMessageAllUnbuffered as u8 || 
                          t == FromClientTCPMessageType::SendMessageGroupUnbuffered as u8 || 
                          t == FromClientTCPMessageType::SendMessageOthersBuffered as u8 || 
                          t == FromClientTCPMessageType::SendMessageAllBuffered as u8 { //others,all,group[t:u8][message.length():i32][message:u8array]
                            match read_send_message(socket.clone(), client.clone(), rooms.clone(), t).await{ 
                                Ok(_)=>(),
                                Err(_)=>{eprintln!("failed to read from socket"); return;}
                            };
                } else if t == FromClientTCPMessageType::CreateGroup as u8 { //[t:u8][list.lengthbytes:i32][clients:i32array]
                    match read_group_message(socket.clone(), client.clone(), clients.clone()).await{ 
                        Ok(_)=>(),
                        Err(_)=>{eprintln!("failed to read from socket"); return;}
                    };
                } else {
                    //die...not correct protocol
                    eprintln!("Incorrect protocol, killing");
                    return;
                }
                
            },
            Err(e) => {
                eprintln!("failed to read from socket; err = {:?}", e);
                //remove the client
                return;
            }
        };        
    }
}
async fn client_write(client: Rc<RefCell<Client>>, mut socket: TcpStream, notify: Rc<Notify>){ 
    
    //wait on messages in my queue
    loop {
        
        notify.notified().await; //there is something to write
        
        let mut to_write = vec![];
        {
            let client_ref = client.borrow();
            to_write.extend_from_slice(&client_ref.message_queue); //must do this so that the borrow ends
        }

        {
            let mut client_ref = client.borrow_mut();
            client_ref.message_queue.clear();
        }

        match socket.write(&to_write).await {
            Ok(_) => (),
            Err(_) => {eprintln!("failed to write to the tcp socket"); return;}
        }

    }
}

async fn client_write_udp(client: Rc<RefCell<Client>>, socket: Rc<RefCell<UdpSocket>>, notify: Rc<Notify>){

    loop {
        notify.notified().await; //there is something to write

        let ip;
        let port;
        let mut messages = vec![];
        {
            let mut client_ref = client.borrow_mut();
            ip = client_ref.ip;
            port = client_ref.udp_port;
            for msg in client_ref.message_queue_udp.iter() {
                messages.push(msg.clone());
            }
            client_ref.message_queue_udp.clear();
        }

        for msg in messages.iter() {
            let socket = socket.borrow();
            match socket.send_to(&msg,SocketAddr::new(ip, port)).await {
                Ok(_)=> (),
                Err(_) => {eprintln!("failed to write to the udp socket"); return;}
            }
        }

    }



}

async fn process_udp(socket: Rc<RefCell<UdpSocket>>,clients: Rc<RefCell<HashMap<u32, Rc<RefCell<Client>>>>>, rooms: Rc<RefCell<HashMap<String, Rc<RefCell<Room>>>>>){

    let mut buf = [0u8;1024];
    loop {
        let socket = socket.borrow();
        let res = socket.recv_from(&mut buf).await;
        
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
                
                
                let clients = clients.borrow();
                if clients.contains_key(&client_id){

                    let mut client = clients.get(&client_id).unwrap().borrow_mut();
                    client.udp_port = addr.port(); //set the udp port to send data to
                    client.message_queue_udp.push(vec![0]);
                    client.notify_udp.notify();

                }

            } else if t == FromClientUDPMessageType::SendMesssageOthersUnbuffered as u8 { //[3:u8][from:i32][contents:u8array] note that it must fit into the packet of 1024 bytes
                
                    
                let clients = clients.borrow();
                if clients.contains_key(&client_id){
                    let client = clients.get(&client_id).unwrap().borrow();
                    let rooms_ref = rooms.borrow();
                    if client.roomname != "" {
                        let room = rooms_ref[&client.roomname].borrow();
                        buf[0] = ToClientUDPMessageType::DataMessage as u8; //technically unecessary, unless we change this number
                        for (_k,v) in room.clients.iter() {
                            if *_k != client_id{
                                let mut msg = vec![];
                                let mut v_ref = v.borrow_mut();
                                msg.extend_from_slice(&buf[0..packet_size]);
                                v_ref.message_queue_udp.push(msg);
                                v_ref.notify_udp.notify();
                            }
                        }
                    }
                }
                
    
            } else if t == FromClientUDPMessageType::SendMessageAllUnbuffered as u8 { //see above
                let clients = clients.borrow();
                if clients.contains_key(&client_id){
                    let mut client = clients.get(&client_id).unwrap().borrow_mut();
                    let rooms_ref = rooms.borrow();
                    if client.roomname != "" {
                        let room = rooms_ref[&client.roomname].borrow();
                        buf[0] = ToClientUDPMessageType::DataMessage as u8; //technically unecessary, unless we change this number
                        for (_k,v) in room.clients.iter() {
                            if *_k != client_id{
                                let mut msg = vec![];
                                let mut v_ref = v.borrow_mut();
                                msg.extend_from_slice(&buf[0..packet_size]);
                                v_ref.message_queue_udp.push(msg);
                                v_ref.notify_udp.notify();
                            }else{
                                let mut msg = vec![];
                                msg.extend_from_slice(&buf[0..packet_size]);
                                client.message_queue_udp.push(msg);
                                client.notify_udp.notify();
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

                let clients = clients.borrow();
                if clients.contains_key(&client_id){
                    let client = clients.get(&client_id).unwrap().borrow();
                    if client.groups.contains_key(&group_name) {
                        let clients = client.groups.get(&group_name).unwrap();
                        //we need to form a new message without the group name 
                        let mut message_to_send = vec![];
                        message_to_send.push(ToClientUDPMessageType::DataMessage as u8);
                        message_to_send.extend([buf[1],buf[2],buf[3],buf[4]]);
                        message_to_send.extend(message_bytes);
                        
                        for v in clients.iter() {
                            let mut v_ref = v.borrow_mut();
                            v_ref.message_queue_udp.push(message_to_send.clone());
                            v_ref.notify_udp.notify();
                        }
                    }
                }
            }
        } 
        
    }
}

//this is in response to someone asking to login (this is where usernames and passwords would be processed, in theory)
async fn read_login_message(mut stream: TcpStream, client: Rc<RefCell<Client>>) -> Result<(),Box<dyn Error>>{
    //byte,shortstring,byte,shortstring
    
    let username = read_short_string(&mut stream).await?;
    let application = read_short_string(&mut stream).await?;
    println!("{}: Got application {} and userid {}",Local::now().format("%Y-%m-%d %H:%M:%S"),application,username);
    {
        let mut client = client.borrow_mut();
        client.username = username;
        client.application = application;
        client.logged_in = true;
    }
    

    
    {
        let mut client = client.borrow_mut();
        let mut write_buf = vec![];
        write_buf.push(ToClientTCPMessageType::LoggedIn as u8);
        write_buf.extend_from_slice(&(client.id).to_be_bytes()); //send the client the id
        client.message_queue.extend(write_buf);
        client.notify.notify();
    }
    return Ok(());
}

async fn read_rooms_message(mut stream: TcpStream, client: Rc<RefCell<Client>>, rooms: Rc<RefCell<HashMap<String, Rc<RefCell<Room>>>>>)-> Result<(),Box<dyn Error>>{

    let mut write_buf = vec![];
    write_buf.push(ToClientTCPMessageType::RoomList as u8);
    //first we need to get the room names

    let rooms = rooms.borrow();
    let mut client = client.borrow_mut();
    let mut rooms_vec = vec![];
    for (k,v) in rooms.iter() {

        
        if !k.starts_with(&client.application.to_string()) {
            continue;
        }
        let mut iter = k.chars();
        
        iter.by_ref().nth(client.application.len());
        let application_stripped_room = iter.as_str();

        let room_string = format!("{}:{}",application_stripped_room,v.borrow().clients.len());
        rooms_vec.push(room_string);
    }
    
    let rooms_message = rooms_vec.join(",");
    let message_bytes = rooms_message.as_bytes();
    let message_len = message_bytes.len() as u32;
    write_buf.extend_from_slice(&(message_len).to_be_bytes());
    write_buf.extend_from_slice(message_bytes);
    client.message_queue.extend_from_slice(&write_buf);
    client.notify.notify();
    return Ok(());
}

async fn read_join_message(mut stream: TcpStream, client: Rc<RefCell<Client>>, rooms: Rc<RefCell<HashMap<String, Rc<RefCell<Room>>>>>)-> Result<(),Box<dyn Error>>{
    //byte,shortstring

    let short_room_name = read_short_string(&mut stream).await?;
    let extended_room_name;
    let mut leave_room = false;
    {
        let client_ref = client.borrow();
        extended_room_name = format!("{}_{}", client_ref.application, short_room_name);
        if client_ref.roomname != "" {
            leave_room = true;
        }
    }
    if leave_room {
        //todo
        client_leave_room(client.clone(), true, rooms.clone()); 
    }

    let mut client_ref = client.borrow_mut();
    let mut rooms_ref = rooms.borrow_mut();

    if short_room_name.trim() == "" || short_room_name == "-1" {
        return Ok(());
    }

    if !rooms_ref.contains_key(&extended_room_name) { //new room, must create it
        let map: HashMap<u32, Rc<RefCell<Client>>> = HashMap::new();
        let r = Rc::new(RefCell::new(Room {
            name: extended_room_name.to_string(),
            clients: map,
            master_client: client.clone() //client is the master, since they joined first
        }));
        client_ref.is_master = true;
        rooms_ref.insert(String::from(&extended_room_name),r);
        println!("{}: {}: New room {} created",Local::now().format("%Y-%m-%d %H:%M:%S"), client_ref.application,&extended_room_name);
    }else{
        client_ref.is_master = false;
    }

    //the room is guaranteed to exist now, so this call can't fail
    let mut room_to_join = rooms_ref[&extended_room_name].borrow_mut();
    room_to_join.clients.insert(client_ref.id,client.clone());
    println!("{}: {}: Client {} joined {}",Local::now().format("%Y-%m-%d %H:%M:%S"), client_ref.application, client_ref.id,&extended_room_name);

    client_ref.roomname = extended_room_name; //we create an option and assign it back to the room
       
    //send a join message to everyone in the room (except the client)
    for (_k,v) in room_to_join.clients.iter() {
        if *_k != client_ref.id {
            send_client_join_message(v, client_ref.id, &short_room_name);
        }
    }

    //send a join message to the client that has all of the ids in the room
    let mut ids_in_room = vec![];  
    for (_k,v) in room_to_join.clients.iter() {
        ids_in_room.push(*_k);
        
    }
    send_you_joined_message(&mut *client_ref, ids_in_room, &short_room_name);

    if client_ref.is_master {
        let temp = client_ref.id;
        send_client_master_message(&mut *client_ref, temp);
    }else{
        send_client_master_message(&mut *client_ref, room_to_join.master_client.borrow().id);
    }

    return Ok(());
}

async fn read_roomdata_message(mut stream: TcpStream, client: Rc<RefCell<Client>>, rooms: Rc<RefCell<HashMap<String, Rc<RefCell<Room>>>>>)-> Result<(),Box<dyn Error>>{
    //type, room_name
    //will respond with type, numclients u32, id1 u32, name_len u8, name_bytes ...

    //read the room name and append the client application

    

    let short_room_name = read_short_string(&mut stream).await?;
    let mut client_ref = client.borrow_mut();
    let room_name = format!("{}_{}", client_ref.application, short_room_name);

    //we need to access the rooms list
    let rooms_ref = rooms.borrow();
    if rooms_ref.contains_key(&room_name) { 

        let room = rooms_ref.get(&room_name).unwrap();
        //form and send the message
        let mut write_buf = vec![];
        write_buf.push(ToClientTCPMessageType::RoomData as u8);

        
        let roomname_bytes = short_room_name.as_bytes();
        write_buf.push(roomname_bytes.len() as u8);
        write_buf.extend_from_slice(&roomname_bytes);

        let clients = &room.borrow().clients;
        write_buf.extend_from_slice(&(clients.len() as u32).to_be_bytes());
        for (_k,c) in clients.iter() {
            write_buf.extend_from_slice(&(_k).to_be_bytes());

            if *_k != client_ref.id {
                let c_ref = c.borrow();
                let username_bytes = c_ref.username.as_bytes();
                write_buf.push(username_bytes.len() as u8);
                write_buf.extend_from_slice(&username_bytes);

            }else{
                let username_bytes = client_ref.username.as_bytes();
                write_buf.push(username_bytes.len() as u8);
                write_buf.extend_from_slice(&username_bytes);
            }

        }
        client_ref.message_queue.extend_from_slice(&write_buf);
        client_ref.notify.notify();
    }

    return Ok(());

}

async fn read_send_message(mut stream: TcpStream, client: Rc<RefCell<Client>>, rooms: Rc<RefCell<HashMap<String, Rc<RefCell<Room>>>>>, message_type: u8)-> Result<(),Box<dyn Error>>{
    //4 byte length, array
    //this is a message for everyone in the room (maybe)
    let to_send = read_vec(&mut stream).await?;
    if message_type == FromClientTCPMessageType::SendMessageOthersUnbuffered as u8 {
       send_room_message(client,&to_send,rooms.clone(),false,false);
    }else if message_type == FromClientTCPMessageType::SendMessageAllUnbuffered as u8 {
       send_room_message(client,&to_send,rooms.clone(),true,false);
    } else if message_type == FromClientTCPMessageType::SendMessageOthersBuffered as u8 { //ordered
        send_room_message(client,&to_send,rooms.clone(),false,true);
     }else if message_type == FromClientTCPMessageType::SendMessageAllBuffered as u8 { //ordered
        send_room_message(client,&to_send,rooms.clone(),true,true);
     }else if message_type == FromClientTCPMessageType::SendMessageGroupUnbuffered as u8 {
       let group = read_short_string(&mut stream).await?;
       send_group_message(client,&to_send, &group);
    }
    return Ok(());
}

async fn read_group_message(mut stream: TcpStream, client: Rc<RefCell<Client>>, clients: Rc<RefCell<HashMap<u32, Rc<RefCell<Client>>>>>)-> Result<(),Box<dyn Error>>{
    
    let group = read_short_string(&mut stream).await?;
    let id_bytes = read_vec(&mut stream).await?;
    let num = id_bytes.len();
    
    let mut client_ref = client.borrow_mut();

    let clients_ref = clients.borrow();
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
        

        match clients_ref.get(&id) {
            Some(client) => {group_clients.push(client.clone());},
            None => () //not there, so don't add it
        }
        
        i = i + 4;
    }

    //delete the group if it exists
    if client_ref.groups.contains_key(&group) {
        client_ref.groups.remove(&group); //ensures the client references go away
    }
    
    client_ref.groups.insert(group.clone(),group_clients); 

    return Ok(());
}

fn client_leave_room(client: Rc<RefCell<Client>>, send_to_client: bool, rooms: Rc<RefCell<HashMap<String, Rc<RefCell<Room>>>>>){
    //first remove the client from the room they are in
    
    {
        
        let mut client_ref = client.borrow_mut();
        let roomname = String::from(client_ref.roomname.clone());

        if roomname == "" { //client not in room, leave
            return;
        }
        let mut new_master_id = 0;
        {
            println!("{}: {}: Client {} in room, leaving",Local::now().format("%Y-%m-%d %H:%M:%S"), client_ref.application,client_ref.id);
        }

        
        let mut destroy_room = false;
        {
            let mut rooms_ref = rooms.borrow_mut();
            let mut room_ref = rooms_ref.get(&roomname).unwrap().borrow_mut();

            for (_k,v) in room_ref.clients.iter() {
                if *_k != client_ref.id {              
                    send_client_left_message(v, client_ref.id, &roomname); 
                }
            }

            if send_to_client && client_ref.roomname != "" { 
                send_you_left_message(&mut *client_ref, &roomname);
            }
        
            room_ref.clients.remove(&client_ref.id); //remove the client from that list in the room
            if room_ref.clients.len() == 0 {
                destroy_room = true;
            }
        }
        //if the room is empty, destroy it as well
        let mut rooms_ref = rooms.borrow_mut();
        if destroy_room {
            
            rooms_ref.remove(&roomname);
            println!("{}: {}: Destroyed room {}",Local::now().format("%Y-%m-%d %H:%M:%S"), client_ref.application, &roomname)
        }else if client_ref.is_master{ //we need to change the master!
            let mut room_ref = rooms_ref.get(&roomname).unwrap().borrow_mut();
            for (_k,v) in room_ref.clients.iter() {
                if *_k != client_ref.id {
                    new_master_id = v.borrow().id;
                    break;
                }
            }

            println!("{}: {}: Changing master to {}",Local::now().format("%Y-%m-%d %H:%M:%S"),client_ref.application, new_master_id);
            
            for (_k,v) in room_ref.clients.iter() {
                let mut c = v.borrow_mut();
                send_client_master_message(&mut *c, new_master_id);
            }
            room_ref.master_client = room_ref.clients.get(&new_master_id).unwrap().clone();
        }
    }
    let mut client_ref = client.borrow_mut();
    client_ref.roomname = String::from("");
}

fn send_you_left_message(client_ref: &mut Client, room: &str){
    let mut write_buf = vec![];
    write_buf.push(ToClientTCPMessageType::YouLeft as u8);
    write_buf.push(room.as_bytes().len() as u8); 
    write_buf.extend_from_slice(room.as_bytes());
    client_ref.message_queue.extend_from_slice(&write_buf);
    client_ref.notify.notify();
}
    
fn send_client_left_message(to: &Rc<RefCell<Client>>, from: u32, room: &str){
    let mut write_buf = vec![];
    write_buf.push(ToClientTCPMessageType::PlayerLeft as u8);
    write_buf.extend_from_slice(&(from).to_be_bytes()); //send everyone that the client id left the room
    write_buf.push(room.as_bytes().len() as u8); 
    write_buf.extend_from_slice(room.as_bytes());
    let mut client_ref = to.borrow_mut();
    client_ref.message_queue.extend_from_slice(&write_buf);
    client_ref.notify.notify();
}

fn send_client_join_message(to: &Rc<RefCell<Client>>, from: u32, room: &str){
    //2u8, person_id_u32, room_name_len_u8, room_name_bytes
    let mut write_buf = vec![];
    write_buf.push(ToClientTCPMessageType::PlayerJoined as u8);
    write_buf.extend_from_slice(&(from).to_be_bytes()); //send everyone that the client id joined the room
    write_buf.push(room.as_bytes().len() as u8); 
    write_buf.extend_from_slice(room.as_bytes());
    let mut client_ref = to.borrow_mut();
    client_ref.message_queue.extend_from_slice(&write_buf);
    client_ref.notify.notify();
    
}

fn send_you_joined_message(client_ref: &mut Client, in_room: Vec<u32>, room: &str){
    //you_joined_u8, ids_len_u32, id_list_array_u32, room_name_len_u8, room_name_bytes
    let mut write_buf = vec![];
    write_buf.push(ToClientTCPMessageType::YouJoined as u8);
    write_buf.extend_from_slice(&(in_room.len() as u32).to_be_bytes());  
    for id in in_room {
        write_buf.extend_from_slice(&(id).to_be_bytes());
    }
    write_buf.push(room.as_bytes().len() as u8); 
    write_buf.extend_from_slice(room.as_bytes());
    client_ref.message_queue.extend_from_slice(&write_buf);
    client_ref.notify.notify();
}

fn send_client_master_message(client_ref: &mut Client, master_id: u32){
    //2u8, person_id_u32, room_name_len_u8, room_name_bytes
    let mut write_buf = vec![];
    write_buf.push(ToClientTCPMessageType::MasterMessage as u8);
    write_buf.extend_from_slice(&(master_id).to_be_bytes()); //send everyone that the client id joined the room
    client_ref.message_queue.extend_from_slice(&write_buf);
    client_ref.notify.notify();
  
}

fn send_room_message(sender: Rc<RefCell<Client>>, message: &Vec<u8>, rooms: Rc<RefCell<HashMap<String, Rc<RefCell<Room>>>>>, include_sender: bool, ordered: bool){
    //this message is 3u8, sender_id_u32, message_len_u32, message_bytes
    
    let mut write_buf = vec![];

    
    let mut sender_ref = sender.borrow_mut();
    write_buf.push(ToClientTCPMessageType::DataMessage as u8);
    write_buf.extend_from_slice(&sender_ref.id.to_be_bytes());
    write_buf.extend_from_slice(&(message.len() as u32).to_be_bytes());
    write_buf.extend_from_slice(message);


    if sender_ref.roomname=="" {
        return;
    }


    if include_sender {
        sender_ref.message_queue.extend_from_slice(&write_buf);
        sender_ref.notify.notify();
    }
    
    let rooms_ref = rooms.borrow();
    let room_ref = rooms_ref[&sender_ref.roomname].borrow();

    for (_k,v) in room_ref.clients.iter(){
        if !include_sender && *_k == sender_ref.id {
            continue;
        }
        
        let mut temp_mut = v.borrow_mut();
        temp_mut.message_queue.extend_from_slice(&write_buf);
        temp_mut.notify.notify();
        
    }
    
        
    
}
fn send_group_message(sender: Rc<RefCell<Client>>, message: &Vec<u8>, group: &String){
    let mut write_buf = vec![];
    let sender_ref = sender.borrow();
    write_buf.push(ToClientTCPMessageType::DataMessage as u8);
    write_buf.extend_from_slice(&sender_ref.id.to_be_bytes());
    write_buf.extend_from_slice(&(message.len() as u32).to_be_bytes());
    write_buf.extend_from_slice(message);

    //get the list of client ids for this group
    let groups = &sender_ref.groups;
    if groups.contains_key(group) {
      let group = groups.get(group).unwrap();
      for c in group {
        let mut temp_mut = c.borrow_mut();
        temp_mut.message_queue.extend_from_slice(&write_buf);
        temp_mut.notify.notify();
      }
    }

}

async fn read_u8(stream: &mut TcpStream) -> Result<u8,Box<dyn Error>> {
    let mut buf = [0; 1];
    stream.read_exact(&mut buf).await?;
    return Ok(buf[0]);
}
async fn read_u32(stream: &mut TcpStream) -> Result<u32,Box<dyn Error>> {
    let mut buf:[u8;4] = [0; 4];
    stream.read_exact(&mut buf).await?;
    let size = u32::from_be_bytes(buf);
    return Ok(size);
}
async fn _read_string(stream: &mut TcpStream) -> Result<String,Box<dyn Error>> {
    let size = read_u32(stream).await?;
    let mut string_bytes = vec![0;size as usize];
    stream.read_exact(&mut string_bytes).await?;
    return Ok(String::from_utf8(string_bytes).unwrap());
}
async fn read_short_string(stream: &mut TcpStream) -> Result<String,Box<dyn Error>> {
    let size = read_u8(stream).await?;
    let mut string_bytes = vec![0;size as usize];
    stream.read_exact(&mut string_bytes).await?;
    return Ok(String::from_utf8(string_bytes).unwrap());
}

async fn read_vec(stream: &mut TcpStream) -> Result<Vec<u8>,Box<dyn Error>> {
    let message_size = read_u32(stream).await?;
    let mut message = vec![0u8;message_size as usize];
    stream.read_exact(&mut message).await?;
    return Ok(message);
}

