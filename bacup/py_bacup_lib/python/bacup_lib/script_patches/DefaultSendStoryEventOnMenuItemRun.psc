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
            Quest trainingCourse = GetEN05TrainingCourse()
            If trainingCourse != None
                If !trainingCourse.IsRunning() && akTerminalRef != None
                    ReferenceAlias activatingTerminal = trainingCourse.GetAlias(1) as ReferenceAlias
                    If activatingTerminal != None
                        activatingTerminal.ForceRefTo(akTerminalRef)
                    EndIf
                    Bool started = entry.StoryEventToSend.SendStoryEventAndWait(eventLocation, akTerminalRef, playerRef, entry.Value1)
                    If !started && !trainingCourse.IsRunning() && activatingTerminal != None
                        activatingTerminal.Clear()
                    EndIf
                EndIf
            Else
                entry.StoryEventToSend.SendStoryEventAndWait(eventLocation, playerRef, akTerminalRef, entry.Value1)
                If directStartAllowed && fallbackQuest != None && !fallbackQuest.IsRunning()
                    fallbackQuest.Start()
                EndIf
            EndIf
        EndIf
        index += 1
    EndWhile
EndEvent

Quest Function GetEN05TrainingCourse()
    If Self == Game.GetFormFromFile(0x00182129, "SeventySix.esm")
        Return Game.GetFormFromFile(0x0008C881, "SeventySix.esm") as Quest
    ElseIf Self == Game.GetFormFromFile(0x001820DD, "SeventySix.esm")
        Return Game.GetFormFromFile(0x0009C824, "SeventySix.esm") as Quest
    ElseIf Self == Game.GetFormFromFile(0x001820CF, "SeventySix.esm")
        Return Game.GetFormFromFile(0x0008D23B, "SeventySix.esm") as Quest
    EndIf
    Return None
EndFunction
