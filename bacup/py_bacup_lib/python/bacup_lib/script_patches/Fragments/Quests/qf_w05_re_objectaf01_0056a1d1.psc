Function Fragment_Stage_0010_Item_00()
    If W05_RE_ObjectAF01_Sandbox != None
        W05_RE_ObjectAF01_Sandbox.Start()
    EndIf
EndFunction

Function Fragment_Stage_0023_Item_00()
    If W05_RE_ObjectAF01_Attack != None
        W05_RE_ObjectAF01_Attack.Start()
    EndIf
EndFunction

Function Fragment_Stage_0025_Item_00()
    ObjectReference protRef = Alias_DisProtectron.GetReference()
    If protRef != None && W05_RE_ObjectAF01_Protectron_Destruct != None
        W05_RE_ObjectAF01_Protectron_Destruct.Cast(protRef, protRef)
    EndIf
    If W05_RE_ObjectAF01_Explosion != None
        W05_RE_ObjectAF01_Explosion.Start()
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    ObjectReference protRef = Alias_DisProtectron.GetReference()
    If protRef != None && W05_RE_ObjectAF01_BrokenProtectronKeyword != None
        protRef.RemoveKeyword(W05_RE_ObjectAF01_BrokenProtectronKeyword)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
EndFunction
