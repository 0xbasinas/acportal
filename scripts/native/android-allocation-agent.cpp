// Opt-in acceptance-only JVMTI probe. Reports stack symbols and class counts,
// never object values, references, credentials or message text.
#include <jvmti.h>
#include <atomic>
#include <fstream>
#include <map>
#include <mutex>
#include <string>
#include <vector>

static jvmtiEnv* ti = nullptr;
static std::mutex lock;
static std::atomic<unsigned long> serial{0};
static std::atomic<unsigned long> samples{0};
static std::atomic<jlong> tags{1LL << 40};
static std::map<std::string, std::pair<unsigned long, unsigned long>> sites;
static std::map<jlong, std::string> objects;
static thread_local bool busy = false;
static std::map<jlong, std::string> class_names;
static std::map<jlong, std::string> inventory_objects;
static std::map<std::string, unsigned long> owner_counts;
static jint JNICALL reference(jvmtiHeapReferenceKind kind, const jvmtiHeapReferenceInfo*, jlong,
    jlong referrer_class, jlong, jlong* target, jlong*, jint, void*) {
    auto object = inventory_objects.find(*target);
    if (object != inventory_objects.end()) {
        auto owner = class_names.find(referrer_class);
        std::string name = owner == class_names.end() ? "root-or-unknown" : owner->second;
        if (owner_counts.size() < 4096) ++owner_counts[object->second + "\t" + name + "\t" + std::to_string(kind)];
    }
    return JVMTI_VISIT_OBJECTS;
}
static jint JNICALL inventory(jlong class_tag, jlong, jlong* tag, jint, void*) {
    auto klass = class_names.find(class_tag);
    if (klass != class_names.end() && (klass->second == "Ldev/acportal/data/LiveSession;" || klass->second == "Ldev/acportal/protocol/SessionState;")) {
        if (!*tag) *tag = ++tags;
        inventory_objects[*tag] = klass->second;
    }
    return JVMTI_VISIT_OBJECTS;
}

static std::string signature(jclass klass) {
    char* text = nullptr;
    if (ti->GetClassSignature(klass, &text, nullptr) != JVMTI_ERROR_NONE) return "unknown";
    std::string result(text); ti->Deallocate(reinterpret_cast<unsigned char*>(text));
    return result;
}
static void JNICALL freed(jvmtiEnv*, jlong tag) {
    std::lock_guard<std::mutex> guard(lock); objects.erase(tag);
}
static void JNICALL allocated(jvmtiEnv*, JNIEnv* env, jthread thread, jobject object, jclass klass, jlong size) {
    if (busy) return;
    busy = true;
    auto index = ++serial;
    bool sampled = index % 16 == 0;
    auto name = signature(klass);
    if (name == "Ldev/acportal/data/LiveSession;" || name == "Ldev/acportal/protocol/SessionState;") {
        jlong previous = 0;
        if (ti->GetTag(object, &previous) == JVMTI_ERROR_NONE && previous == 0) {
            jlong tag = ++tags;
            if (ti->SetTag(object, tag) == JVMTI_ERROR_NONE) {
                std::lock_guard<std::mutex> guard(lock); objects[tag] = name;
            }
        }
    }
    if (sampled) {
        jvmtiFrameInfo frames[48]; jint count = 0;
        std::string site = name + "\tframework-or-no-app-frame";
        if (ti->GetStackTrace(thread, 0, 48, frames, &count) == JVMTI_ERROR_NONE) {
            for (int i = 0; i < count; ++i) {
                jclass declaring = nullptr;
                if (ti->GetMethodDeclaringClass(frames[i].method, &declaring) != JVMTI_ERROR_NONE) continue;
                auto owner = signature(declaring);
                env->DeleteLocalRef(declaring);
                if (owner.rfind("Ldev/acportal/", 0) != 0) continue;
                char* method = nullptr;
                if (ti->GetMethodName(frames[i].method, &method, nullptr, nullptr) != JVMTI_ERROR_NONE) continue;
                jint entries = 0; jvmtiLineNumberEntry* lines = nullptr; jint line = -1;
                if (ti->GetLineNumberTable(frames[i].method, &entries, &lines) == JVMTI_ERROR_NONE) {
                    for (int j = 0; j < entries; ++j) if (lines[j].start_location <= frames[i].location) line = lines[j].line_number;
                    ti->Deallocate(reinterpret_cast<unsigned char*>(lines));
                }
                site = name + "\t" + owner + "." + method + ":" + std::to_string(line);
                ti->Deallocate(reinterpret_cast<unsigned char*>(method)); break;
            }
        }
        std::lock_guard<std::mutex> guard(lock);
        if (sites.size() < 4096 || sites.count(site)) {
            auto& record = sites[site]; ++record.first; record.second += size; ++samples;
        }
    }
    busy = false;
}
extern "C" JNIEXPORT jint JNICALL Agent_OnAttach(JavaVM* vm, char*, void*) {
    if (ti || vm->GetEnv(reinterpret_cast<void**>(&ti), JVMTI_VERSION_1_2) != JNI_OK) return JNI_ERR;
    jvmtiCapabilities capabilities{};
    capabilities.can_generate_vm_object_alloc_events = 1;
    capabilities.can_generate_object_free_events = 1;
    capabilities.can_tag_objects = 1;
    capabilities.can_get_line_numbers = 1;
    if (ti->AddCapabilities(&capabilities) != JVMTI_ERROR_NONE) return JNI_ERR;
    jvmtiEventCallbacks callbacks{}; callbacks.VMObjectAlloc = allocated; callbacks.ObjectFree = freed;
    if (ti->SetEventCallbacks(&callbacks, sizeof(callbacks)) != JVMTI_ERROR_NONE) return JNI_ERR;
    if (ti->SetEventNotificationMode(JVMTI_ENABLE, JVMTI_EVENT_OBJECT_FREE, nullptr) != JVMTI_ERROR_NONE) return JNI_ERR;
    return ti->SetEventNotificationMode(JVMTI_ENABLE, JVMTI_EVENT_VM_OBJECT_ALLOC, nullptr) == JVMTI_ERROR_NONE ? JNI_OK : JNI_ERR;
}
extern "C" JNIEXPORT jstring JNICALL Java_dev_acportal_data_NativeAllocationProbe_report(JNIEnv* env, jobject, jstring path, jboolean finish) {
    if (!ti) return env->NewStringUTF("probe-unavailable");
    ti->SetEventNotificationMode(JVMTI_DISABLE, JVMTI_EVENT_VM_OBJECT_ALLOC, nullptr);
    if (ti->ForceGarbageCollection() != JVMTI_ERROR_NONE) return env->NewStringUTF("probe-gc-failed");
    // Inventory all currently reachable instances, including allocations that ART
    // does not expose through VMObjectAlloc, and record only direct owner classes.
    jint classes = 0; jclass* loaded = nullptr;
    class_names.clear();
    if (ti->GetLoadedClasses(&classes, &loaded) == JVMTI_ERROR_NONE) {
        for (jint i = 0; i < classes; ++i) {
            jlong tag = -static_cast<jlong>(i + 100);
            if (ti->SetTag(loaded[i], tag) != JVMTI_ERROR_NONE) return env->NewStringUTF("probe-class-tag-failed");
            class_names[tag] = signature(loaded[i]);
            env->DeleteLocalRef(loaded[i]);
        }
        ti->Deallocate(reinterpret_cast<unsigned char*>(loaded));
    } else return env->NewStringUTF("probe-loaded-classes-failed");
    inventory_objects.clear(); owner_counts.clear();
    jvmtiHeapCallbacks callbacks{};
    callbacks.heap_iteration_callback = inventory;
    if (ti->IterateThroughHeap(0, nullptr, &callbacks, nullptr) != JVMTI_ERROR_NONE) return env->NewStringUTF("probe-inventory-failed");
    callbacks.heap_iteration_callback = nullptr;
    callbacks.heap_reference_callback = reference;
    if (ti->FollowReferences(0, nullptr, nullptr, &callbacks, nullptr) != JVMTI_ERROR_NONE) return env->NewStringUTF("probe-owners-failed");
    std::map<std::string, unsigned long> retained;
    const char* file = env->GetStringUTFChars(path, nullptr);
    {
        std::lock_guard<std::mutex> guard(lock);
        for (const auto& object : inventory_objects) ++retained[object.second];
        std::ofstream output(file);
        output << "Sampling: every 16th VM allocation; bounded to 4096 sites; framework/test overhead included\n";
        for (const auto& row : sites) output << row.first << "\t" << row.second.first << "\t" << row.second.second << "\n";
        for (const auto& row : retained) output << "retained\t" << row.first << "\t" << row.second << "\n";
        for (const auto& row : owner_counts) output << "directOwner\t" << row.first << "\t" << row.second << "\n";
        output << "observedAllocations\t" << serial << "\nsampledAllocations\t" << samples << "\n";
    }
    env->ReleaseStringUTFChars(path, file);
    if (finish) ti->SetEventNotificationMode(JVMTI_DISABLE, JVMTI_EVENT_OBJECT_FREE, nullptr);
    else ti->SetEventNotificationMode(JVMTI_ENABLE, JVMTI_EVENT_VM_OBJECT_ALLOC, nullptr);
    auto result = "samples=" + std::to_string(samples.load()) + ";liveSessions=" + std::to_string(retained["Ldev/acportal/data/LiveSession;"]) + ";sessionStates=" + std::to_string(retained["Ldev/acportal/protocol/SessionState;"]);
    return env->NewStringUTF(result.c_str());
}
