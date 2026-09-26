Function ShowLocalCipher(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || SiloGroupID < 0 || SiloGroupID > 2
        Return
    EndIf
    Quest masterQuest = Game.GetFormFromFile(0x003CD064, "SeventySix.esm") as Quest
    Nuke_MasterScript master = masterQuest as Nuke_MasterScript
    If master != None && B21:KeypadNative.Ready()
        Debug.MessageBox(master.LocalCipherBriefing(SiloGroupID))
    EndIf
EndFunction

Event OnActivate(ObjectReference akActionRef)
    ShowLocalCipher(akActionRef)
EndEvent

State StartsWaiting
    Event OnActivate(ObjectReference akActionRef)
        ShowLocalCipher(akActionRef)
    EndEvent
EndState

State waiting
    Event OnActivate(ObjectReference akActionRef)
        ShowLocalCipher(akActionRef)
    EndEvent
EndState
