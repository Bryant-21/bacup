; @drop-member OnMenuItemRun

Function RegisterTerminalEvents()
    Terminal stationTerminal = GetBaseObject() as Terminal
    If stationTerminal != None
        RegisterForRemoteEvent(stationTerminal, "OnMenuItemRun")
    EndIf
    TW002_Script wardenQuest = Game.GetFormFromFile(0x0010E201, "SeventySix.esm") as TW002_Script
    If wardenQuest != None && wardenQuest.IsRunning()
        wardenQuest.RegisterForStationEvents()
    EndIf
EndFunction

Event OnInit()
    RegisterTerminalEvents()
EndEvent

Event OnLoad()
    RegisterTerminalEvents()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        RegisterTerminalEvents()
    EndIf
EndEvent

Event Terminal.OnMenuItemRun(Terminal akSender, Int auiMenuItemID, ObjectReference akTerminalRef)
    If akSender == GetBaseObject() as Terminal && akTerminalRef == Self && auiMenuItemID == MenuID
        If TW002Terminal != None && HasKeyword(TW002Terminal)
            Self.SendCustomEvent("TW002GotTape", None)
        EndIf
    EndIf
EndEvent
