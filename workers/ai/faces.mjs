// YuNet decoding follows OpenCV's Apache-2.0 face_detect.cpp contract.
// Reference: https://github.com/opencv/opencv/blob/4.x/modules/objdetect/src/face_detect.cpp
import { Tensor } from 'onnxruntime-node';
const clamp = (v,a,b) => Math.min(b,Math.max(a,v));
function rgb(image,x,y,c) { const xx=clamp(x,0,image.width-1), yy=clamp(y,0,image.height-1); const at=(yy*image.width+xx)*image.channels; return image.data[at+Math.min(c,image.channels-1)]; }
function sample(image,x,y,c) { const ix=Math.floor(x),iy=Math.floor(y),dx=x-ix,dy=y-iy;return rgb(image,ix,iy,c)*(1-dx)*(1-dy)+rgb(image,ix+1,iy,c)*dx*(1-dy)+rgb(image,ix,iy+1,c)*(1-dx)*dy+rgb(image,ix+1,iy+1,c)*dx*dy; }
function iou(a,b) { const x=Math.max(a.box[0],b.box[0]),y=Math.max(a.box[1],b.box[1]);const r=Math.min(a.box[0]+a.box[2],b.box[0]+b.box[2]),bottom=Math.min(a.box[1]+a.box[3],b.box[1]+b.box[3]);const intersection=Math.max(0,r-x)*Math.max(0,bottom-y);return intersection/(a.box[2]*a.box[3]+b.box[2]*b.box[3]-intersection+1e-9); }
function alignedTensor(image,points) {
    const target=[[38.2946,51.6963],[73.5318,51.5014],[56.0252,71.7366],[41.5493,92.3655],[70.7299,92.2041]];
    const mean=(p,c)=>p.reduce((s,v)=>s+v[c],0)/p.length;
    const sx=mean(points,0),sy=mean(points,1),dx=mean(target,0),dy=mean(target,1);
    let numeratorA=0,numeratorB=0,denominator=0;
    for(let i=0;i<5;i++){const x=points[i][0]-sx,y=points[i][1]-sy,u=target[i][0]-dx,v=target[i][1]-dy;numeratorA+=x*u+y*v;numeratorB+=x*v-y*u;denominator+=x*x+y*y;}
    if(denominator<1e-9)throw new Error('人脸关键点无效');
    const a=numeratorA/denominator,b=numeratorB/denominator,tx=dx-a*sx+b*sy,ty=dy-b*sx-a*sy,scale=a*a+b*b;
    const input=new Float32Array(3*112*112);
    for(let y=0;y<112;y++)for(let x=0;x<112;x++){const u=x-tx,v=y-ty;const ox=(a*u+b*v)/scale,oy=(-b*u+a*v)/scale;for(let c=0;c<3;c++)input[c*112*112+y*112+x]=sample(image,ox,oy,c);}
    return new Tensor('float32',input,[1,3,112,112]);
}
export async function recognizeFaces(image,detector,recognizer) {
    const edge=640,scale=Math.min(edge/image.width,edge/image.height),w=Math.round(image.width*scale),h=Math.round(image.height*scale);
    const input=new Float32Array(3*edge*edge);
    for(let y=0;y<h;y++)for(let x=0;x<w;x++)for(let c=0;c<3;c++)input[c*edge*edge+y*edge+x]=sample(image,x/scale,y/scale,2-c);
    const outputs=await detector.run({[detector.inputNames[0]]:new Tensor('float32',input,[1,3,edge,edge])});
    const detections=[];
    for(const stride of [8,16,32]){
        const cols=edge/stride,cls=outputs[`cls_${stride}`].data,obj=outputs[`obj_${stride}`].data,boxes=outputs[`bbox_${stride}`].data,landmarks=outputs[`kps_${stride}`].data;
        for(let y=0;y<cols;y++)for(let x=0;x<cols;x++){const i=y*cols+x,score=Math.sqrt(clamp(cls[i],0,1)*clamp(obj[i],0,1));if(score<0.85)continue;
            const cx=(x+boxes[i*4])*stride/scale,cy=(y+boxes[i*4+1])*stride/scale,bw=Math.exp(boxes[i*4+2])*stride/scale,bh=Math.exp(boxes[i*4+3])*stride/scale;
            if(![cx,cy,bw,bh].every(Number.isFinite)||bw<12||bh<12)continue;
            const points=Array.from({length:5},(_,j)=>[(landmarks[i*10+j*2]+x)*stride/scale,(landmarks[i*10+j*2+1]+y)*stride/scale]);
            detections.push({box:[cx-bw/2,cy-bh/2,bw,bh],points,score});
        }
    }
    detections.sort((a,b)=>b.score-a.score);const kept=[];
    for(const face of detections){if(kept.some(old=>iou(old,face)>0.3))continue;kept.push(face);if(kept.length===64)break;}
    const result=[];
    for(const face of kept){const output=await recognizer.run({[recognizer.inputNames[0]]:alignedTensor(image,face.points)});const vector=Array.from(output[recognizer.outputNames[0]].data,Number);const norm=Math.hypot(...vector);if(!norm||!Number.isFinite(norm))continue;
        result.push({box:[clamp(face.box[0]/image.width,0,1),clamp(face.box[1]/image.height,0,1),clamp(face.box[2]/image.width,0,1),clamp(face.box[3]/image.height,0,1)],score:face.score,vector:vector.map(v=>v/norm)});
    }
    return result;
}
