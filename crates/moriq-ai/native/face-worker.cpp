// CPU-only YuNet/SFace worker. Framed stdin/stdout keeps paths and image data separate.
#include <opencv2/opencv.hpp>
#include <opencv2/objdetect/face.hpp>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <sstream>
#include <iomanip>
#include <cstdint>
#include <io.h>
#include <fcntl.h>

static std::vector<uchar> load(const wchar_t* path) {
    std::ifstream file(std::filesystem::path(path), std::ios::binary);
    if (!file) throw std::runtime_error("Cannot open bundled face model");
    return {std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>()};
}
static bool usable(const cv::Mat& image, const cv::Mat& face) {
    float w=face.at<float>(0,2), h=face.at<float>(0,3);
    if (std::min(w,h)<64 || w*h/(image.cols*image.rows)<0.004f) return false;
    cv::Rect box(cvRound(face.at<float>(0,0)),cvRound(face.at<float>(0,1)),cvRound(w),cvRound(h));
    box &= cv::Rect(0,0,image.cols,image.rows);
    if (box.width<2 || box.height<2) return false;
    cv::Mat gray, resized, lap;
    cv::cvtColor(image(box),gray,cv::COLOR_BGR2GRAY);
    cv::resize(gray,resized,cv::Size(96,96),0,0,cv::INTER_LINEAR);
    cv::Laplacian(resized,lap,CV_64F);
    cv::Scalar mean, dev; cv::meanStdDev(lap,mean,dev);
    return dev[0]*dev[0]>=20;
}
int wmain(int argc, wchar_t** argv) {
    _setmode(_fileno(stdin),_O_BINARY); _setmode(_fileno(stdout),_O_BINARY);
    try {
        if (argc!=3) return 2;
        cv::setNumThreads(2);
        auto detector=cv::FaceDetectorYN::create("onnx",load(argv[1]),{},cv::Size(320,320),0.85f,0.3f,5000,cv::dnn::DNN_BACKEND_OPENCV,cv::dnn::DNN_TARGET_CPU);
        auto recognizer=cv::FaceRecognizerSF::create("onnx",load(argv[2]),{},cv::dnn::DNN_BACKEND_OPENCV,cv::dnn::DNN_TARGET_CPU);
        uint32_t length=0;
        while (std::cin.read(reinterpret_cast<char*>(&length),4)) {
            if (!length || length>16*1024*1024) return 3;
            std::vector<uchar> bytes(length);
            if (!std::cin.read(reinterpret_cast<char*>(bytes.data()),length)) return 4;
            auto start=cv::getTickCount();
            cv::Mat image=cv::imdecode(bytes,cv::IMREAD_COLOR);
            if (image.empty() || image.cols>1600 || image.rows>1600) return 5;
            detector->setInputSize(image.size());
            cv::Mat faces; detector->detect(image,faces);
            std::ostringstream output; output<<std::setprecision(9)<<"{\"faces\":[";
            int accepted=0;
            for(int i=0;i<faces.rows && accepted<64;i++) {
                auto face=faces.row(i);
                if(!usable(image,face)) continue;
                cv::Mat aligned,feature;
                recognizer->alignCrop(image,face,aligned); recognizer->feature(aligned,feature);
                cv::normalize(feature,feature);
                float x=std::max(0.f,face.at<float>(0,0))/image.cols;
                float y=std::max(0.f,face.at<float>(0,1))/image.rows;
                float w=std::min(face.at<float>(0,2)/image.cols,1.f-x);
                float h=std::min(face.at<float>(0,3)/image.rows,1.f-y);
                if(w<=0 || h<=0) continue;
                if(accepted++) output<<',';
                output<<"{\"box\":["<<x<<','<<y<<','<<w<<','<<h<<"],\"vector\":[";
                for(size_t j=0;j<feature.total();j++) {if(j)output<<',';output<<feature.ptr<float>()[j];}
                output<<"]}";
            }
            output<<"],\"detected\":"<<faces.rows<<",\"backend\":\"opencv-cpu\",\"inference_ms\":"<<(cv::getTickCount()-start)*1000.0/cv::getTickFrequency()<<'}';
            auto data=output.str(); length=static_cast<uint32_t>(data.size());
            std::cout.write(reinterpret_cast<const char*>(&length),4); std::cout.write(data.data(),length); std::cout.flush();
        }
        return 0;
    } catch(const std::exception& e) {std::cerr<<e.what()<<'\n';return 6;}
}
