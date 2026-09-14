Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || MenuData == None
        Return
    EndIf

    Int index = 0
    While index < MenuData.Length
        MenuDatum entry = MenuData[index]
        Quest fallbackQuest = entry.UniqueQuestToAddPlayer
        If fallbackQuest == None
            fallbackQuest = entry.ExpectedQuestToStart
        EndIf
        Bool directStartAllowed = entry.ExpectedQuestToStart != None && entry.UniqueQuestToAddPlayer == entry.ExpectedQuestToStart
        Bool menuMatches = entry.iMenuItemTarget == auiMenuItemID
        Bool terminalMatches = !entry.bCheckforCurrentTerminalObject || (akTerminalRef != None && akTerminalRef.GetBaseObject() == Self)
        Bool playerDoesNotHaveQuest = entry.ActiveQuestKeyword == None || !playerRef.HasKeyword(entry.ActiveQuestKeyword)
        Bool playerIsNotBlocked = entry.BlockingActorValue == None || playerRef.GetValue(entry.BlockingActorValue) < entry.iBlockingValue
        Bool boundQuestNeedsStart = fallbackQuest == None || !fallbackQuest.IsRunning()

        If menuMatches && terminalMatches && playerDoesNotHaveQuest && playerIsNotBlocked && boundQuestNeedsStart && entry.StoryEventToSend != None
            Location eventLocation = entry.LocToSend
            If eventLocation == None
                eventLocation = playerRef.GetCurrentLocation()
            EndIf
            entry.StoryEventToSend.SendStoryEventAndWait(eventLocation, playerRef, akTerminalRef, entry.Value1)
            If directStartAllowed && fallbackQuest != None && !fallbackQuest.IsRunning()
                fallbackQuest.Start()
            EndIf
        EndIf
        index += 1
    EndWhile
EndEvent
