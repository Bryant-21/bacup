Function Fragment_Stage_0030_Item_00()
    Int index = 0
    While Alias_RefColl_AMS01_Robots != None && index < Alias_RefColl_AMS01_Robots.GetCount()
        ObjectReference encounterRef = Alias_RefColl_AMS01_Robots.GetAt(index)
        If encounterRef != None
            encounterRef.DisableNoWait()
        EndIf
        index += 1
    EndWhile

    index = 0
    While Alias_RefColl_AMS02_Actors != None && index < Alias_RefColl_AMS02_Actors.GetCount()
        ObjectReference actorWaveRef = Alias_RefColl_AMS02_Actors.GetAt(index)
        If actorWaveRef != None
            actorWaveRef.DisableNoWait()
        EndIf
        index += 1
    EndWhile

    index = 0
    While Alias_RefColl_AMS02_Robots != None && index < Alias_RefColl_AMS02_Robots.GetCount()
        ObjectReference robotWaveRef = Alias_RefColl_AMS02_Robots.GetAt(index)
        If robotWaveRef != None
            robotWaveRef.DisableNoWait()
        EndIf
        index += 1
    EndWhile

    index = 0
    While Alias_Actors_MercenaryGuards != None && index < Alias_Actors_MercenaryGuards.GetCount()
        ObjectReference guardRef = Alias_Actors_MercenaryGuards.GetAt(index)
        If guardRef != None
            guardRef.DisableNoWait()
        EndIf
        index += 1
    EndWhile
EndFunction

Function Fragment_Stage_0040_Item_00()
    If IsStageDone(1450)
        Alias_Actor_Marcia_AMS.TryToDisableNoWait()
        Alias_Actor_Marcia_AMSBasement.TryToEnableNoWait()
        Alias_Actor_Marcia_AMSBasement.TryToEvaluatePackage()

        Int index = 0
        While Alias_Actors_MercenaryGuards != None && index < Alias_Actors_MercenaryGuards.GetCount()
            ObjectReference guardRef = Alias_Actors_MercenaryGuards.GetAt(index)
            If guardRef != None
                guardRef.EnableNoWait()
            EndIf
            index += 1
        EndWhile
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
    If BS02_MarciaAwayValue != None
        Alias_Player.TryToSetValue(BS02_MarciaAwayValue, 1.0)
    EndIf
    If BS02_SheenaAwayValue != None
        Alias_Player.TryToSetValue(BS02_SheenaAwayValue, 1.0)
    EndIf
    If BS02_BurkeAwayValue != None
        Alias_Player.TryToSetValue(BS02_BurkeAwayValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(50, True)
    If BS02_MarciaWarRoomValue != None
        Alias_Player.TryToSetValue(BS02_MarciaWarRoomValue, 1.0)
    EndIf
    Alias_Actor_Marcia_WarRoom.TryToEnableNoWait()
    Alias_Actor_Marcia_WarRoom.TryToEvaluatePackage()
EndFunction

Function Fragment_Stage_0550_Item_00()
    If BS02_MQ02_Missing_MarciaPierceWarRoomAmbient != None && !BS02_MQ02_Missing_MarciaPierceWarRoomAmbient.IsPlaying()
        BS02_MQ02_Missing_MarciaPierceWarRoomAmbient.Start()
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(50, True)
    SetObjectiveDisplayed(60, True)
    If BS02_MarciaWarRoomValue != None
        Alias_Player.TryToSetValue(BS02_MarciaWarRoomValue, 0.0)
    EndIf
    Alias_Actor_Marcia_WarRoom.TryToDisableNoWait()
    Alias_Actor_Marcia_AMSFirstLevel.TryToEnableNoWait()
    Alias_Actor_Marcia_AMSFirstLevel.TryToEvaluatePackage()
EndFunction

Function Fragment_Stage_0650_Item_00()
    SetObjectiveCompleted(60, True)
    SetObjectiveDisplayed(65, True)
    Alias_Actor_Marcia_AMSFirstLevel.TryToEnableNoWait()
    Alias_Actor_Mercenary.TryToEnableNoWait()

    Actor marciaRef = Alias_Actor_Marcia_AMSFirstLevel.GetActorReference()
    Actor mercenaryRef = Alias_Actor_Mercenary.GetActorReference()
    If mercenaryRef != None && BS02_HellcatMercenariesFaction != None && !mercenaryRef.IsInFaction(BS02_HellcatMercenariesFaction)
        mercenaryRef.AddToFaction(BS02_HellcatMercenariesFaction)
    EndIf
    If mercenaryRef != None && marciaRef != None && !mercenaryRef.IsDead()
        mercenaryRef.StartCombat(marciaRef)
    EndIf
EndFunction

Function Fragment_Stage_0675_Item_00()
    SetObjectiveCompleted(65, True)
    SetObjectiveDisplayed(68, True)
    If z_BS02_MQ02_Missing_Marcia01 != None && !z_BS02_MQ02_Missing_Marcia01.IsPlaying()
        z_BS02_MQ02_Missing_Marcia01.Start()
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(68, True)
    SetObjectiveDisplayed(67, True)
    SetObjectiveDisplayed(69, True)
    SetObjectiveDisplayed(70, True)
    Alias_Actor_Marcia_AMSFirstLevel.TryToDisableNoWait()
    Alias_Actor_Marcia_AMS.TryToEnableNoWait()
    Alias_Actor_Marcia_AMS.TryToEvaluatePackage()
    Alias_Marker_ClueEnable.TryToEnableNoWait()

    Int index = 0
    While Alias_RefColl_ThirdFloorMercs != None && index < Alias_RefColl_ThirdFloorMercs.GetCount()
        ObjectReference mercenaryRef = Alias_RefColl_ThirdFloorMercs.GetAt(index)
        If mercenaryRef != None
            mercenaryRef.EnableNoWait()
        EndIf
        index += 1
    EndWhile

    index = 0
    While Alias_RefColl_AMS01_Robots != None && index < Alias_RefColl_AMS01_Robots.GetCount()
        ObjectReference robotRef = Alias_RefColl_AMS01_Robots.GetAt(index)
        If robotRef != None
            robotRef.EnableNoWait()
        EndIf
        index += 1
    EndWhile

    index = 0
    While Alias_RadiationHazards != None && index < Alias_RadiationHazards.GetCount()
        ObjectReference radiationRef = Alias_RadiationHazards.GetAt(index)
        If radiationRef != None
            radiationRef.EnableNoWait()
        EndIf
        index += 1
    EndWhile
EndFunction

Function Fragment_Stage_0750_Item_00()
    Alias_Marker_ClueEnable.TryToEnableNoWait()
EndFunction

Function Fragment_Stage_0755_Item_00()
    SetObjectiveCompleted(67, True)
EndFunction

Function Fragment_Stage_0760_Item_00()
    If IsStageDone(770) && !IsStageDone(775)
        SetStage(775)
    EndIf
EndFunction

Function Fragment_Stage_0770_Item_00()
    If IsStageDone(760) && !IsStageDone(775)
        SetStage(775)
    EndIf
EndFunction

Function Fragment_Stage_0775_Item_00()
    SetObjectiveCompleted(69, True)
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(70, True)
    SetObjectiveDisplayed(75, True)
    Alias_Activator_BurkesNecklace.TryToDisableNoWait()
    Alias_Marker_MarciaNecklace.TryToDisableNoWait()

    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && BS02_MQ02_Missing_BurkesNecklace != None && playerRef.GetItemCount(BS02_MQ02_Missing_BurkesNecklace) < 1
        playerRef.AddItem(BS02_MQ02_Missing_BurkesNecklace, 1, False)
    EndIf
    If z_BS02_MQ02_Missing_Marcia02 != None && !z_BS02_MQ02_Missing_Marcia02.IsPlaying()
        z_BS02_MQ02_Missing_Marcia02.Start()
    EndIf
EndFunction

Function Fragment_Stage_0850_Item_00()
    SetObjectiveCompleted(75, True)
    SetObjectiveDisplayed(70, True)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(70, True)
    SetObjectiveDisplayed(75, True)
    If z_BS02_MQ02_Missing_Marcia03 != None && !z_BS02_MQ02_Missing_Marcia03.IsPlaying()
        z_BS02_MQ02_Missing_Marcia03.Start()
    EndIf
EndFunction

Function Fragment_Stage_0950_Item_00()
    SetObjectiveCompleted(75, True)
    SetObjectiveDisplayed(70, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(70, True)
    SetObjectiveDisplayed(75, True)
    Alias_Activator_SheenasNote.TryToDisableNoWait()
    Alias_Marker_MarciaNote.TryToDisableNoWait()

    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && BS02_MQ02_Missing_TerminalPassword != None && playerRef.GetItemCount(BS02_MQ02_Missing_TerminalPassword) < 1
        playerRef.AddItem(BS02_MQ02_Missing_TerminalPassword, 1, False)
    EndIf
    If z_BS02_MQ02_Missing_Marcia04 != None && !z_BS02_MQ02_Missing_Marcia04.IsPlaying()
        z_BS02_MQ02_Missing_Marcia04.Start()
    EndIf
EndFunction

Function Fragment_Stage_1025_Item_00()
    Alias_Activator_SheenasNote.TryToDisableNoWait()
    Alias_Marker_MarciaNote.TryToDisableNoWait()
EndFunction

Function Fragment_Stage_1050_Item_00()
    SetObjectiveCompleted(75, True)
    SetObjectiveDisplayed(80, True)
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveDisplayed(80, True)
EndFunction

Function Fragment_Stage_1175_Item_00()
    SetObjectiveCompleted(80, True)
    SetObjectiveDisplayed(82, True)
    SetObjectiveDisplayed(85, True)

    ObjectReference topFloorDoor = Alias_Door_TopFloor.GetRef()
    If topFloorDoor != None
        topFloorDoor.Lock(False)
        topFloorDoor.SetOpen(True)
    EndIf

    Int index = 0
    While Alias_RefColl_TopFloorMercs != None && index < Alias_RefColl_TopFloorMercs.GetCount()
        ObjectReference mercenaryRef = Alias_RefColl_TopFloorMercs.GetAt(index)
        If mercenaryRef != None
            mercenaryRef.EnableNoWait()
        EndIf
        index += 1
    EndWhile
EndFunction

Function Fragment_Stage_1180_Item_00()
    SetObjectiveCompleted(82, True)
    SetObjectiveDisplayed(85, True)
    If zBS02_MQ02_Missing_Kit01 != None && !zBS02_MQ02_Missing_Kit01.IsPlaying()
        zBS02_MQ02_Missing_Kit01.Start()
    EndIf
EndFunction

Function Fragment_Stage_1183_Item_00()
    Int index = 0
    While Alias_RefColl_AMS02_Actors != None && index < Alias_RefColl_AMS02_Actors.GetCount()
        ObjectReference waveActorRef = Alias_RefColl_AMS02_Actors.GetAt(index)
        If waveActorRef != None
            waveActorRef.EnableNoWait()
        EndIf
        index += 1
    EndWhile
EndFunction

Function Fragment_Stage_1185_Item_00()
    If zBS02_MQ02_Missing_Kit02 != None && !zBS02_MQ02_Missing_Kit02.IsPlaying()
        zBS02_MQ02_Missing_Kit02.Start()
    EndIf
EndFunction

Function Fragment_Stage_1190_Item_00()
    Int index = 0
    While Alias_RefColl_AMS02_Robots != None && index < Alias_RefColl_AMS02_Robots.GetCount()
        ObjectReference waveRobotRef = Alias_RefColl_AMS02_Robots.GetAt(index)
        If waveRobotRef != None
            waveRobotRef.EnableNoWait()
        EndIf
        index += 1
    EndWhile
EndFunction

Function Fragment_Stage_1195_Item_00()
    If zBS02_MQ02_Missing_Kit03 != None && !zBS02_MQ02_Missing_Kit03.IsPlaying()
        zBS02_MQ02_Missing_Kit03.Start()
    EndIf
EndFunction

Function Fragment_Stage_1198_Item_00()
    SetObjectiveCompleted(85, True)
    SetObjectiveDisplayed(90, True)
    Alias_Actor_Kit.TryToEnableNoWait()

    ObjectReference kitDoor = Alias_Door_KitsRoom.GetRef()
    If kitDoor != None
        kitDoor.Lock(False)
        kitDoor.SetOpen(True)
    EndIf

    Actor kitRef = Alias_Actor_Kit.GetActorReference()
    Actor playerRef = Alias_Player.GetActorReference()
    If kitRef != None && BS02_HellcatMercenariesFaction != None && !kitRef.IsInFaction(BS02_HellcatMercenariesFaction)
        kitRef.AddToFaction(BS02_HellcatMercenariesFaction)
    EndIf
    If kitRef != None && playerRef != None && !kitRef.IsDead()
        kitRef.StartCombat(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(90, True)
    SetObjectiveDisplayed(100, True)
    Alias_Marker_TopFloorRespawn.TryToDisableNoWait()
EndFunction

Function Fragment_Stage_1250_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(105, True)

    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && BS02_MQ02_Missing_BlackburnsLetter != None && playerRef.GetItemCount(BS02_MQ02_Missing_BlackburnsLetter) < 1
        playerRef.AddItem(BS02_MQ02_Missing_BlackburnsLetter, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveCompleted(105, True)
    SetObjectiveDisplayed(110, True)
    Alias_Container_KitDesk.TryToEnableNoWait()
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveCompleted(110, True)
    SetObjectiveDisplayed(115, True)

    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && BS02_MQ02_Missing_BasementKey != None && playerRef.GetItemCount(BS02_MQ02_Missing_BasementKey) < 1
        playerRef.AddItem(BS02_MQ02_Missing_BasementKey, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_1425_Item_00()
    SetObjectiveCompleted(115, True)
    SetObjectiveDisplayed(120, True)

    ObjectReference elevatorDoor = Alias_Door_AMSTopFloorElevator.GetRef()
    If elevatorDoor != None
        elevatorDoor.Lock(False)
        elevatorDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_1450_Item_00()
    SetObjectiveCompleted(120, True)
    SetObjectiveDisplayed(130, True)
    Alias_Actor_Marcia_AMS.TryToDisableNoWait()
    Alias_Actor_Marcia_AMSBasement.TryToEnableNoWait()
    Alias_Actor_Marcia_AMSBasement.TryToEvaluatePackage()
    Alias_Marker_BasementScene.TryToEnableNoWait()

    ObjectReference basementDoor = Alias_Door_Basement.GetRef()
    If basementDoor != None
        basementDoor.Lock(False)
    EndIf

    Int index = 0
    While Alias_Actors_MercenaryGuards != None && index < Alias_Actors_MercenaryGuards.GetCount()
        ObjectReference guardRef = Alias_Actors_MercenaryGuards.GetAt(index)
        If guardRef != None
            guardRef.EnableNoWait()
        EndIf
        index += 1
    EndWhile

    index = 0
    While Alias_RadiationHazards != None && index < Alias_RadiationHazards.GetCount()
        ObjectReference radiationRef = Alias_RadiationHazards.GetAt(index)
        If radiationRef != None
            radiationRef.DisableNoWait()
        EndIf
        index += 1
    EndWhile
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveCompleted(130, True)
    SetObjectiveDisplayed(140, True)
EndFunction

Function Fragment_Stage_1550_Item_00()
    If z_BS02_MQ02_Missing_MarciaBasement != None && !z_BS02_MQ02_Missing_MarciaBasement.IsPlaying()
        z_BS02_MQ02_Missing_MarciaBasement.Start()
    EndIf
EndFunction

Function Fragment_Stage_1560_Item_00()
    If BS02_MQ02_Missing_SheenaBurkeAMS != None && !BS02_MQ02_Missing_SheenaBurkeAMS.IsPlaying()
        BS02_MQ02_Missing_SheenaBurkeAMS.Start()
    EndIf
EndFunction

Function Fragment_Stage_1570_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        If BS02_MQ02_Missing_BurkesNecklace != None && playerRef.GetItemCount(BS02_MQ02_Missing_BurkesNecklace) > 0
            playerRef.RemoveItem(BS02_MQ02_Missing_BurkesNecklace, 1, True)
        EndIf
        If BS02_MQ02_Missing_GaveBurkeNecklace != None
            playerRef.SetValue(BS02_MQ02_Missing_GaveBurkeNecklace, 1.0)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1575_Item_00()
    SetObjectiveCompleted(140, True)
    SetObjectiveDisplayed(150, True)
    SetObjectiveDisplayed(155, True)
    SetObjectiveDisplayed(156, True)
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetObjectiveDisplayed(156, False)
    SetObjectiveDisplayed(158, True)
EndFunction

Function Fragment_Stage_1650_Item_00()
    SetObjectiveDisplayed(150, True)
EndFunction

Function Fragment_Stage_1675_Item_00()
    SetObjectiveCompleted(150, True)

    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && BS02_MQ02_Missing_ResearchNotes != None && playerRef.GetItemCount(BS02_MQ02_Missing_ResearchNotes) < 1
        playerRef.AddItem(BS02_MQ02_Missing_ResearchNotes, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_1700_Item_00()
    ObjectReference sheenaDoor = Alias_Door_SheenasCell.GetRef()
    If sheenaDoor != None
        sheenaDoor.Lock(False)
        sheenaDoor.SetOpen(True)
    EndIf
    ObjectReference burkeDoor = Alias_Door_BurkesCell.GetRef()
    If burkeDoor != None
        burkeDoor.Lock(False)
        burkeDoor.SetOpen(True)
    EndIf

    Actor sheenaRef = Alias_Actor_Sheena.GetActorReference()
    ReferenceAlias burkeAlias = GetAlias(12) as ReferenceAlias
    Actor burkeRef = None
    If burkeAlias != None
        burkeRef = burkeAlias.GetActorReference()
    EndIf
    If sheenaRef != None
        If BoundCaptiveFaction != None && sheenaRef.IsInFaction(BoundCaptiveFaction)
            sheenaRef.RemoveFromFaction(BoundCaptiveFaction)
        EndIf
        sheenaRef.EvaluatePackage()
    EndIf
    If burkeRef != None
        If BoundCaptiveFaction != None && burkeRef.IsInFaction(BoundCaptiveFaction)
            burkeRef.RemoveFromFaction(BoundCaptiveFaction)
        EndIf
        burkeRef.EvaluatePackage()
    EndIf
    If zBS02_MQ02_Missing_Sheena != None && !zBS02_MQ02_Missing_Sheena.IsPlaying()
        zBS02_MQ02_Missing_Sheena.Start()
    EndIf
EndFunction

Function Fragment_Stage_1750_Item_00()
    SetObjectiveCompleted(155, True)
    SetObjectiveCompleted(158, True)
    SetObjectiveDisplayed(150, False)
    SetObjectiveDisplayed(156, False)
    SetObjectiveDisplayed(160, True)
    SetObjectiveDisplayed(165, True)
    If BS02_SheenaAwayValue != None
        Alias_Player.TryToSetValue(BS02_SheenaAwayValue, 0.0)
    EndIf
    If BS02_BurkeAwayValue != None
        Alias_Player.TryToSetValue(BS02_BurkeAwayValue, 0.0)
    EndIf
EndFunction

Function Fragment_Stage_1775_Item_00()
    SetObjectiveCompleted(156, True)
    SetObjectiveDisplayed(150, False)
    SetObjectiveDisplayed(155, False)
    SetObjectiveDisplayed(158, False)
    SetObjectiveDisplayed(160, True)
    SetObjectiveDisplayed(165, True)
EndFunction

Function Fragment_Stage_1800_Item_00()
    SetObjectiveDisplayed(160, True)
    SetObjectiveDisplayed(165, True)
EndFunction

Function Fragment_Stage_1850_Item_00()
    SetObjectiveCompleted(165, True)
    SetObjectiveDisplayed(160, False)
    SetObjectiveDisplayed(170, True)
    If BS02_MarciaAwayValue != None
        Alias_Player.TryToSetValue(BS02_MarciaAwayValue, 1.0)
    EndIf
    If BS02_MarciaWarRoomValue != None
        Alias_Player.TryToSetValue(BS02_MarciaWarRoomValue, 1.0)
    EndIf
    Alias_Actor_Marcia_AMSBasement.TryToDisableNoWait()
    Alias_Actor_Marcia_WarRoom.TryToEnableNoWait()
    Alias_Actor_Marcia_WarRoom.TryToEvaluatePackage()
EndFunction

Function Fragment_Stage_1875_Item_00()
    SetObjectiveCompleted(160, True)
    SetObjectiveDisplayed(165, False)
    SetObjectiveDisplayed(170, True)
    If BS02_MarciaAwayValue != None
        Alias_Player.TryToSetValue(BS02_MarciaAwayValue, 0.0)
    EndIf
    If BS02_MarciaWarRoomValue != None
        Alias_Player.TryToSetValue(BS02_MarciaWarRoomValue, 0.0)
    EndIf
    Alias_Actor_Marcia_AMSBasement.TryToDisableNoWait()
EndFunction

Function Fragment_Stage_1900_Item_00()
    SetObjectiveCompleted(170, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(170, True)

    Alias_Marker_ClueEnable.TryToDisableNoWait()
    Alias_Marker_BasementScene.TryToDisableNoWait()
    Alias_Marker_TopFloorRespawn.TryToDisableNoWait()

    Int index = 0
    While Alias_RadiationHazards != None && index < Alias_RadiationHazards.GetCount()
        ObjectReference radiationRef = Alias_RadiationHazards.GetAt(index)
        If radiationRef != None
            radiationRef.DisableNoWait()
        EndIf
        index += 1
    EndWhile

    Actor playerRef = Alias_Player.GetActorReference()
    CompleteQuest()

    If playerRef != None && BS02_MQ03_Blue != None && !BS02_MQ03_Blue.IsRunning() && !BS02_MQ03_Blue.IsCompleted() && BS02_MQ03_Blue_QuestStartKeyword != None
        Bool storyAccepted = BS02_MQ03_Blue_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        If storyAccepted
            Return
        EndIf
        If !BS02_MQ03_Blue.IsRunning() && !BS02_MQ03_Blue.IsCompleted()
            StartTimer(5.0, 9000)
        EndIf
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 9000 && IsStageDone(9000)
        Actor playerRef = Alias_Player.GetActorReference()
        If playerRef != None && BS02_MQ03_Blue != None && !BS02_MQ03_Blue.IsRunning() && !BS02_MQ03_Blue.IsCompleted() && BS02_MQ03_Blue_QuestStartKeyword != None
            Bool storyAccepted = BS02_MQ03_Blue_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
            If storyAccepted
                Return
            EndIf
            If !BS02_MQ03_Blue.IsRunning() && !BS02_MQ03_Blue.IsCompleted()
                StartTimer(5.0, 9000)
            EndIf
        EndIf
    EndIf
EndEvent
