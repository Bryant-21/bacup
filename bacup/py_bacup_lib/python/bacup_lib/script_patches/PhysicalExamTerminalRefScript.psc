; @drop-member OnMenuItemRun

Function RegisterExamTerminal()
    Terminal terminalBase = GetBaseObject() as Terminal
    If terminalBase != None && terminalBase == Game.GetFormFromFile(0x00065BDD, "SeventySix.esm")
        RegisterForRemoteEvent(terminalBase, "OnMenuItemRun")
    EndIf
EndFunction

Event OnInit()
    RegisterExamTerminal()
EndEvent

Event OnLoad()
    RegisterExamTerminal()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        RegisterExamTerminal()
    EndIf
EndEvent

Event Terminal.OnMenuItemRun(Terminal akSender, Int auiMenuItemID, ObjectReference akTerminalRef)
    If auiMenuItemID != 1 || akTerminalRef != Self || akSender != Game.GetFormFromFile(0x00065BDD, "SeventySix.esm")
        Return
    EndIf

    Quest physicalExam = Game.GetFormFromFile(0x0000D783, "SeventySix.esm") as Quest
    Keyword startKeyword = Game.GetFormFromFile(0x0006EFC6, "SeventySix.esm") as Keyword
    Location examLocation = Game.GetFormFromFile(0x002E5A6F, "SeventySix.esm") as Location
    Actor playerRef = Game.GetPlayer()
    If physicalExam == None || startKeyword == None || examLocation == None || playerRef == None || physicalExam.IsRunning()
        Return
    EndIf

    If physicalExam.GetStage() > 0
        physicalExam.Reset()
    EndIf

    ReferenceAlias activatingTerminal = physicalExam.GetAlias(1) as ReferenceAlias
    If activatingTerminal == None
        Return
    EndIf
    activatingTerminal.ForceRefTo(akTerminalRef)

    Bool started = startKeyword.SendStoryEventAndWait(examLocation, akTerminalRef, playerRef)
    If !started && !physicalExam.IsRunning()
        activatingTerminal.Clear()
    EndIf
EndEvent
