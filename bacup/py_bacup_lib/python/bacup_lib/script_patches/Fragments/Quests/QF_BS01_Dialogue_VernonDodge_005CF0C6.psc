Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && AV_Relationship != None
        playerRef.SetValue(AV_Relationship, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && AV_Relationship != None
        playerRef.SetValue(AV_Relationship, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && AV_Relationship != None
        playerRef.SetValue(AV_Relationship, 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && AV_Relationship != None
        playerRef.SetValue(AV_Relationship, 4.0)
    EndIf
EndFunction
