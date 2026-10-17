Function Fragment_Stage_0010_Item_00()
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape01PickedUp, 1.0)
    EndIf
    SetObjectiveDisplayed(15)
EndFunction

Function Fragment_Stage_0015_Item_00()
    If IsObjectiveDisplayed(15) && !IsObjectiveCompleted(15)
        SetObjectiveCompleted(15, True)
    EndIf
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape01Played, 1.0)
    EndIf
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0020_Item_00()
    If IsObjectiveDisplayed(20) && !IsObjectiveCompleted(20)
        SetObjectiveCompleted(20, True)
    EndIf
    If IsObjectiveDisplayed(15) && !IsObjectiveCompleted(15)
        SetObjectiveCompleted(15, True)
    EndIf
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape02PickedUp, 1.0)
    EndIf
    SetObjectiveDisplayed(25)
EndFunction

Function Fragment_Stage_0025_Item_00()
    If IsObjectiveDisplayed(25) && !IsObjectiveCompleted(25)
        SetObjectiveCompleted(25, True)
    EndIf
    If IsObjectiveDisplayed(20) && !IsObjectiveCompleted(20)
        SetObjectiveCompleted(20, True)
    EndIf
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape02Played, 1.0)
    EndIf
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0030_Item_00()
    If IsObjectiveDisplayed(30) && !IsObjectiveCompleted(30)
        SetObjectiveCompleted(30, True)
    EndIf
    If IsObjectiveDisplayed(25) && !IsObjectiveCompleted(25)
        SetObjectiveCompleted(25, True)
    EndIf
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape03PickedUp, 1.0)
    EndIf
    SetObjectiveDisplayed(35)
EndFunction

Function Fragment_Stage_0035_Item_00()
    If IsObjectiveDisplayed(35) && !IsObjectiveCompleted(35)
        SetObjectiveCompleted(35, True)
    EndIf
    If IsObjectiveDisplayed(30) && !IsObjectiveCompleted(30)
        SetObjectiveCompleted(30, True)
    EndIf
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape03Played, 1.0)
    EndIf
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0040_Item_00()
    If IsObjectiveDisplayed(40) && !IsObjectiveCompleted(40)
        SetObjectiveCompleted(40, True)
    EndIf
    If IsObjectiveDisplayed(35) && !IsObjectiveCompleted(35)
        SetObjectiveCompleted(35, True)
    EndIf
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape04PickedUp, 1.0)
    EndIf
    SetObjectiveDisplayed(45)
EndFunction

Function Fragment_Stage_0045_Item_00()
    If IsObjectiveDisplayed(45) && !IsObjectiveCompleted(45)
        SetObjectiveCompleted(45, True)
    EndIf
    If IsObjectiveDisplayed(40) && !IsObjectiveCompleted(40)
        SetObjectiveCompleted(40, True)
    EndIf
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape04Played, 1.0)
    EndIf
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0050_Item_00()
    If IsObjectiveDisplayed(50) && !IsObjectiveCompleted(50)
        SetObjectiveCompleted(50, True)
    EndIf
    If IsObjectiveDisplayed(45) && !IsObjectiveCompleted(45)
        SetObjectiveCompleted(45, True)
    EndIf
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape05PickedUp, 1.0)
    EndIf
    SetObjectiveDisplayed(55)
EndFunction

Function Fragment_Stage_0055_Item_00()
    If IsObjectiveDisplayed(55) && !IsObjectiveCompleted(55)
        SetObjectiveCompleted(55, True)
    EndIf
    If IsObjectiveDisplayed(50) && !IsObjectiveCompleted(50)
        SetObjectiveCompleted(50, True)
    EndIf
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape05Played, 1.0)
    EndIf
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0060_Item_00()
    If IsObjectiveDisplayed(60) && !IsObjectiveCompleted(60)
        SetObjectiveCompleted(60, True)
    EndIf
    If IsObjectiveDisplayed(55) && !IsObjectiveCompleted(55)
        SetObjectiveCompleted(55, True)
    EndIf
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape06PickedUp, 1.0)
    EndIf
    SetObjectiveDisplayed(65)
EndFunction

Function Fragment_Stage_0065_Item_00()
    If IsObjectiveDisplayed(65) && !IsObjectiveCompleted(65)
        SetObjectiveCompleted(65, True)
    EndIf
    If IsObjectiveDisplayed(60) && !IsObjectiveCompleted(60)
        SetObjectiveCompleted(60, True)
    EndIf
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None
        playerRef.SetValue(OverseerPersonal_Holotape06Played, 1.0)
    EndIf
    SetObjectiveDisplayed(67)
EndFunction

Function Fragment_Stage_0067_Item_00()
    If IsObjectiveDisplayed(67) && !IsObjectiveCompleted(67)
        SetObjectiveCompleted(67, True)
    EndIf
    If IsObjectiveDisplayed(65) && !IsObjectiveCompleted(65)
        SetObjectiveCompleted(65, True)
    EndIf
    SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0070_Item_00()
    If IsObjectiveDisplayed(67) && !IsObjectiveCompleted(67)
        SetObjectiveCompleted(67, True)
    EndIf
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
