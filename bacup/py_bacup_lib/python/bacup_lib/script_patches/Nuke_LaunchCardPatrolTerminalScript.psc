Form Function GetLocalTargetItem(Int aiTargetType, Int aiSiloGroupID)
    If aiTargetType == 1
        Return Game.GetFormFromFile(0x00121CBE, "SeventySix.esm")
    EndIf
    Int firstFormID = 0x003DA648
    If aiSiloGroupID == 1
        firstFormID = 0x004DE233
    ElseIf aiSiloGroupID == 2
        firstFormID = 0x004DE23B
    EndIf
    Return Game.GetFormFromFile(firstFormID + Utility.RandomInt(0, 7), "SeventySix.esm")
EndFunction

ObjectReference Function CreateLocalTarget(ObjectReference akTerminalRef, Form akItem)
    If akTerminalRef == None || akItem == None
        Return None
    EndIf
    Form containerBase = Game.GetFormFromFile(0x003E25D9, "SeventySix.esm")
    ObjectReference targetRef
    If containerBase != None
        targetRef = akTerminalRef.PlaceAtMe(containerBase, 1, False, False, True)
    EndIf
    If targetRef != None
        targetRef.AddItem(akItem, 1, True)
        targetRef.Lock(False, False)
    EndIf
    Return targetRef
EndFunction

ObjectReference Function ResolveLocalCodeTarget(Int aiSiloGroupID, Form akCodePage, ObjectReference akTerminalRef)
    Quest nukeCodesQuest = Game.GetFormFromFile(0x003DA647, "SeventySix.esm") as Quest
    Nuke_CodesScript nukeCodes = nukeCodesQuest as Nuke_CodesScript
    ObjectReference targetRef
    If nukeCodes != None && nukeCodesQuest.IsRunning()
        targetRef = nukeCodes.PrepareLocalCodeTarget(aiSiloGroupID, akCodePage)
    EndIf
    If targetRef == None
        targetRef = CreateLocalTarget(akTerminalRef, akCodePage)
    EndIf
    Return targetRef
EndFunction

Bool Function StartLocalCodeHunt(Int aiTargetType, Int aiSiloGroupID, ObjectReference akTarget)
    If aiTargetType < 0 || aiTargetType > 1 || akTarget == None
        Return False
    EndIf
    Quest codeHuntQuest = Game.GetFormFromFile(0x002D0F6A, "SeventySix.esm") as Quest
    If codeHuntQuest == None || codeHuntQuest.IsRunning()
        Return False
    EndIf
    If codeHuntQuest.IsCompleted()
        codeHuntQuest.Reset()
    EndIf
    If codeHuntQuest.IsRunning() || codeHuntQuest.IsCompleted()
        Return False
    EndIf
    ReferenceAlias targetAlias = codeHuntQuest.GetAlias(0) as ReferenceAlias
    ReferenceAlias target02Alias = codeHuntQuest.GetAlias(7) as ReferenceAlias
    ReferenceAlias playerAlias = codeHuntQuest.GetAlias(1) as ReferenceAlias
    LocationAlias targetLocationAlias = codeHuntQuest.GetAlias(2) as LocationAlias
    If targetAlias != None
        targetAlias.ForceRefTo(akTarget)
    EndIf
    If target02Alias != None
        target02Alias.ForceRefTo(akTarget)
    EndIf
    If playerAlias != None
        playerAlias.ForceRefTo(Game.GetPlayer())
    EndIf
    If targetLocationAlias != None
        targetLocationAlias.ForceLocationTo(akTarget.GetCurrentLocation())
    EndIf
    Bool accepted = EN07_CodeHuntQuestStartKeyword.SendStoryEventAndWait(akTarget.GetCurrentLocation(), akTarget, akTarget, aiTargetType, aiSiloGroupID)
    If !accepted || !codeHuntQuest.IsRunning()
        Return False
    EndIf
    EN07_CodeHuntQuestScript controller = codeHuntQuest as EN07_CodeHuntQuestScript
    If controller != None
        controller.BeginLocalHunt(aiTargetType, aiSiloGroupID, akTarget, akTarget)
    EndIf
    Return True
EndFunction

Function AddSiloMarkers()
    Int i = 0
    While i < MarkersToAdd.Length
        If MarkersToAdd[i] != None
            MarkersToAdd[i].AddToMap()
        EndIf
        i += 1
    EndWhile
    Game.GetPlayer().SetValue(EN07_CodeHuntReceivedSiloLocations, 1.0)
EndFunction

Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
    AddSiloMarkers()
    Int targetType = -1
    Int siloGroupID = -1
    If auiMenuItemID == 1
        targetType = 1
    ElseIf auiMenuItemID >= 2 && auiMenuItemID <= 4
        targetType = 0
        siloGroupID = auiMenuItemID - 2
    Else
        Return
    EndIf

    Form targetItem = GetLocalTargetItem(targetType, siloGroupID)
    ObjectReference targetRef
    If targetType == 0
        targetRef = ResolveLocalCodeTarget(siloGroupID, targetItem, akTerminalRef)
    Else
        targetRef = CreateLocalTarget(akTerminalRef, targetItem)
    EndIf
    StartLocalCodeHunt(targetType, siloGroupID, targetRef)
EndEvent
