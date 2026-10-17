Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None && XPD_AC02_AV_MetMotherCharlotte != None
        playerRef.SetValue(XPD_AC02_AV_MetMotherCharlotte, 1.0)
    EndIf
EndFunction
