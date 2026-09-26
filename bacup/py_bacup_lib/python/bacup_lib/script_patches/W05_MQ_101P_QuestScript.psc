; The three ToTW aliases ship with a NULL forced reference and are Optional, so
; they can only be populated by W05_MQ_101P_A once it has created those actors.
Function FillTopOfTheWorldAliases(ObjectReference akMeg, ObjectReference akRaiderA, ObjectReference akRaiderB)
    If MegAtTopOfTheWorld && akMeg
        MegAtTopOfTheWorld.ForceRefTo(akMeg)
    EndIf
    If RaiderAAtTopOfTheWorld && akRaiderA
        RaiderAAtTopOfTheWorld.ForceRefTo(akRaiderA)
    EndIf
    If RaiderBAtTopOfTheWorld && akRaiderB
        RaiderBAtTopOfTheWorld.ForceRefTo(akRaiderB)
    EndIf
EndFunction

; FO76 set stage 40 ("Talk to the Overseer") from An Ounce of Prevention's
; server-side completion. Watch RS03 once stage 30 has sent the player there.
Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 30
        Quest inoculation = Game.GetFormFromFile(0x0022730F, "SeventySix.esm") as Quest
        If inoculation
            RegisterForRemoteEvent(inoculation, "OnStageSet")
            RegisterForRemoteEvent(inoculation, "OnQuestShutdown")
            TryInoculationHandoff(inoculation)
        EndIf
    EndIf
EndEvent

Function TryInoculationHandoff(Quest akInoculation)
    If akInoculation && akInoculation.IsCompleted() && IsStageDone(30) && !IsStageDone(40) && !IsStageDone(100)
        SetStage(40)
    EndIf
EndFunction

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    TryInoculationHandoff(akSender)
EndEvent

Event Quest.OnQuestShutdown(Quest akSender)
    TryInoculationHandoff(akSender)
EndEvent
