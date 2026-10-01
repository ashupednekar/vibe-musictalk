#import <Foundation/Foundation.h>
#import <Security/Security.h>

// The host and its signed extension share only a short-lived local capture capability.
static NSMutableDictionary *MTSessionQuery(void) {
    NSMutableDictionary *query = [@{(__bridge id)kSecClass:(__bridge id)kSecClassGenericPassword,
        (__bridge id)kSecAttrService:@"dev.musictalk.capture",
        (__bridge id)kSecAttrAccount:@"active-call"} mutableCopy];
    NSString *group = [[NSBundle mainBundle] objectForInfoDictionaryKey:@"MusicTalkKeychainGroup"];
    if (group.length) query[(__bridge id)kSecAttrAccessGroup] = group;
    return query;
}

static NSDictionary *MTReadSession(void) {
    NSMutableDictionary *query = MTSessionQuery();
    query[(__bridge id)kSecReturnData] = @YES;
    query[(__bridge id)kSecMatchLimit] = (__bridge id)kSecMatchLimitOne;
    CFTypeRef result = NULL;
    if (SecItemCopyMatching((__bridge CFDictionaryRef)query, &result) != errSecSuccess) return nil;
    NSData *data = CFBridgingRelease(result);
    return [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
}

static OSStatus MTWriteSession(NSDictionary *session) {
    NSMutableDictionary *query = MTSessionQuery();
    NSData *data = [NSJSONSerialization dataWithJSONObject:session options:0 error:nil];
    OSStatus status = SecItemUpdate((__bridge CFDictionaryRef)query,
        (__bridge CFDictionaryRef)@{(__bridge id)kSecValueData:data});
    if (status != errSecItemNotFound) return status;
    query[(__bridge id)kSecValueData] = data;
    query[(__bridge id)kSecAttrAccessible] = (__bridge id)kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly;
    return SecItemAdd((__bridge CFDictionaryRef)query, NULL);
}

static void MTDeleteSession(void) {
    SecItemDelete((__bridge CFDictionaryRef)MTSessionQuery());
}
