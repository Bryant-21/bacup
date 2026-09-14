Function Fragment_Stage_0010_Item_00()
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape01PickedUp, 1.0)
    EndIf
    SetObjectiveDisplayed(15)
EndFunction

Function Fragment_Stage_0015_Item_00()
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape01Played, 1.0)
    EndIf
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0020_Item_00()
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape02PickedUp, 1.0)
    EndIf
    SetObjectiveDisplayed(25)
EndFunction

Function Fragment_Stage_0025_Item_00()
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape02Played, 1.0)
    EndIf
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0030_Item_00()
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape03PickedUp, 1.0)
    EndIf
    SetObjectiveDisplayed(35)
EndFunction

Function Fragment_Stage_0035_Item_00()
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape03Played, 1.0)
    EndIf
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0040_Item_00()
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape04PickedUp, 1.0)
    EndIf
    SetObjectiveDisplayed(45)
EndFunction

Function Fragment_Stage_0045_Item_00()
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape04Played, 1.0)
    EndIf
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0050_Item_00()
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape05PickedUp, 1.0)
    EndIf
    SetObjectiveDisplayed(55)
EndFunction

Function Fragment_Stage_0055_Item_00()
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape05Played, 1.0)
    EndIf
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0060_Item_00()
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape06PickedUp, 1.0)
    EndIf
    SetObjectiveDisplayed(65)
EndFunction

Function Fragment_Stage_0065_Item_00()
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape06Played, 1.0)
    EndIf
    SetObjectiveDisplayed(67)
EndFunction

Function Fragment_Stage_0067_Item_00()
    SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0070_Item_00()
    ObjectReference spawnMarker = Alias_EvanSpawnMarker.GetReference()
    If spawnMarker != None && Alias_Evan != None && OverseerPersonal_Evan != None
        Actor evanRef = spawnMarker.PlaceAtMe(OverseerPersonal_Evan) as Actor
        If evanRef != None
            Alias_Evan.ForceRefTo(evanRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(70)
EndFunction
