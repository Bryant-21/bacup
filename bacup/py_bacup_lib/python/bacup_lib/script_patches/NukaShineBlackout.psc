; FO76's server moved the blacked-out player to a random BlackoutLocationList marker. The fade starts at once,
; the move happens after EffectStartDelay, and waking up casts the hangover (SuccessSpellToCast).
Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget != Game.GetPlayer()
        Return
    EndIf
    playerTarget = akTarget
    transportResultCode = 0
    If FadeToBlack != None
        FadeToBlack.Apply(1.0)
    EndIf
    StartTimer(EffectStartDelay, 1)
EndEvent

Event OnTimer(Int aiTimerID)
    Transport()
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    If akTarget != Game.GetPlayer()
        Return
    EndIf
    Transport()
    If FadeToBlack != None
        FadeToBlack.Remove()
    EndIf
    If WakeUp != None
        WakeUp.Apply(1.0)
    EndIf
EndEvent

Function Transport()
    If transportResultCode != 0 || playerTarget == None
        Return
    EndIf
    transportResultCode = -1
    ObjectReference destination = PickDestination()
    If destination != None
        playerTarget.MoveTo(destination)
        transportResultCode = 1
        If SuccessSpellToCast != None
            SuccessSpellToCast.Cast(playerTarget, playerTarget)
        EndIf
    EndIf
    Quest nukashineQuest = Game.GetFormFromFile(0x47A44D, "SeventySix.esm") as Quest
    If nukashineQuest != None && nukashineQuest.IsRunning() && nukashineQuest.IsStageDone(400) && !nukashineQuest.IsStageDone(450)
        nukashineQuest.SetStage(450)
    EndIf
EndFunction

ObjectReference Function PickDestination()
    If BlackoutLocationList == None || BlackoutLocationList.Length == 0
        Return None
    EndIf
    Int attempts = 0
    While attempts < BlackoutLocationList.Length
        ObjectReference candidate = BlackoutLocationList[Utility.RandomInt(0, BlackoutLocationList.Length - 1)]
        If candidate != None && candidate.GetParentCell() != playerTarget.GetParentCell()
            Return candidate
        EndIf
        attempts += 1
    EndWhile
    Return None
EndFunction
