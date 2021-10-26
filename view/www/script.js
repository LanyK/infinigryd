
// load JSON data
function getData() {
    var request = new XMLHttpRequest();
    request.open("GET", 'data.json', false);
    request.overrideMimeType('text/json');
    request.send(null);
    console.log("received: ", request.responseText);
    return JSON.parse(JSON.parse(request.responseText));
}

// get playing field dimensions
// function getDimensions(data) {
//     //console.log("keys are ", Object.keys(data));



//     return {
//         x: math.abs(Xmin) + Xmax,
//         y: math.abs(Ymin) + Ymax
//     }
// }

// let data = {
//     "[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]:10.181.171.46": {
//         "position": {
//             "x": 2,
//             "y": 0
//         },
//         "num_figures": 1
//     },
//     "[1, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255]:10.181.171.46": {
//         "position": {
//             "x": 1,
//             "y": -1
//         },
//         "num_figures": 2
//     },
//     "[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]:10.181.171.46": {
//         "position": {
//             "x": 0,
//             "y": 0
//         },
//         "num_figures": 1
//     },
//     "[0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]:10.181.171.46": {
//         "position": {
//             "x": 0,
//             "y": 1
//         },
//         "num_figures": 2
//     },
//     "[0, 0, 0, 0, 0, 0, 0, 0, 254, 255, 255, 255, 255, 255, 255, 255]:10.181.171.46": {
//         "position": {
//             "x": 0,
//             "y": -2
//         },
//         "num_figures": 1
//     },
//     "[2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0]:10.181.171.46": {
//         "position": {
//             "x": 2,
//             "y": 3
//         },
//         "num_figures": 1
//     },
//     "[2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]:10.181.171.46": {
//         "position": {
//             "x": 2,
//             "y": 1
//         },
//         "num_figures": 1
//     },
//     "[254, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0]:10.181.171.46": {
//         "position": {
//             "x": -2,
//             "y": 0
//         },
//         "num_figures": 1
//     }
// };
let data = getData();
//var field = getDimensions(data);

var Xmax = 0;
var Ymax = 0;
var Xmin = 0;
var Ymin = 0;

for (var key in data) {
    if (data[key].position.x > Xmax) Xmax = data[key].position.x;
    if (data[key].position.y > Ymax) Ymax = data[key].position.y;

    if (data[key].position.x < Xmin) Xmin = data[key].position.x;
    if (data[key].position.y < Ymin) Ymin = data[key].position.y;
}

let maxSizeX = Math.max(Math.abs(Xmin), Xmax) * 2 + 1;
let maxSizeY = Math.max(Math.abs(Ymin), Ymax) * 2 + 1;
console.log("maxX maxY", maxSizeX, maxSizeY);

// let's go

var canvas = document.getElementById("field");

console.log("maxsizex", maxSizeX);

canvas.setAttribute("width", window.innerWidth);
canvas.setAttribute("height", window.innerHeight);

var ctx = canvas.getContext("2d");

var actorWidth = Math.floor(window.innerWidth / maxSizeX);
var actorHeight = Math.floor(window.innerHeight / maxSizeY);
console.log("width height ", actorWidth, actorHeight);
ctx.font = "" + actorHeight + "px Arial";

let colourMap = { "10.181.171.46": "rgb(255,0,0)", "141.84.94.111": "rgb(0,255,0)", "141.84.94.207": "rgb(255,0,0)" }
// Yes, that is faster than for..in:
// https://stackoverflow.com/questions/13645890/javascript-for-in-vs-for-loop-performance

for (var key in data) {
    const posX = (data[key].position.x - Xmin) * actorWidth;
    const posY = (data[key].position.y - Ymin) * actorHeight;
    const midX = posX + actorWidth / 2;
    const midY = posY + actorHeight;
    const colour = colourMap[key.substring(key.search(':') + 1)];


    //console.log("do stuff at ", posX, posY);

    //ctx.fillStyle = "rgb(" + data[key].colour[0] + "," + data[i].colour[1] + "," + data[i].colour[2] + ")";
    //ctx.fillStyle = "rgb(255,255,255)";
    //ctx.beginPath();
    ctx.fillStyle = colour;
    ctx.fillRect(posX, posY, actorWidth, actorHeight);

    ctx.fillStyle = "rgb(0,0,0)";

    ctx.fillText(data[key].num_figures, midX, midY);
    //ctx.closePath();
    //ctx.stroke();
}

console.log(field);
