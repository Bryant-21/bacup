; @drop-member OnMenuItemRun

Function RegisterCombatTerminal()
    Terminal terminalBase = GetBaseObject() as Terminal
    If terminalBase != None
        RegisterForRemoteEvent(terminalBase, "OnMenuItemRun")
    EndIf
EndFunction

Event OnInit()
    RegisterCombatTerminal()
EndEvent

Event OnLoad()
    RegisterCombatTerminal()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        RegisterCombatTerminal()
    EndIf
EndEvent

Event Terminal.OnMenuItemRun(Terminal akSender, Int auiMenuItemID, ObjectReference akTerminalRef)
    If akTerminalRef != Self || akSender != GetBaseObject()
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || MenuData == None
        Return
    EndIf

    Int index = 0
    While index < MenuData.Length
        MenuDatum entry = MenuData[index]
        Bool menuMatches = entry.iMenuItemTarget == auiMenuItemID
        Bool playerDoesNotHaveQuest = entry.ActiveQuestKeyword == None || !playerRef.HasKeyword(entry.ActiveQuestKeyword)
        Bool questNeedsStart = entry.QuestToStart == None || !entry.QuestToStart.IsRunning()

        If menuMatches && playerDoesNotHaveQuest && questNeedsStart && entry.StoryEventToSend != None
            ReferenceAlias activatingTerminal
            If entry.QuestToStart != None
                activatingTerminal = entry.QuestToStart.GetAlias(1) as ReferenceAlias
                If activatingTerminal != None
                    activatingTerminal.ForceRefTo(akTerminalRef)
                EndIf
            EndIf
            Bool started = entry.StoryEventToSend.SendStoryEventAndWait(playerRef.GetCurrentLocation(), akTerminalRef, playerRef, entry.Value1)
            If !started && activatingTerminal != None && !entry.QuestToStart.IsRunning()
                activatingTerminal.Clear()
            EndIf
        EndIf
        index += 1
    EndWhile
EndEvent
