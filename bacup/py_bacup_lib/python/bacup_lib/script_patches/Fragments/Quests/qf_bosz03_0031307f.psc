BoSZ03Script Function EventScript()
    Quest owner = Self as Quest
    Return owner as BoSZ03Script
EndFunction

Function ResetEventObjectives()
    ResetEventObjective(100)
    ResetEventObjective(125)
    ResetEventObjective(200)
    ResetEventObjective(250)
    ResetEventObjective(300)
EndFunction

Function ResetEventObjective(Int aiObjective)
    SetObjectiveDisplayed(aiObjective, False)
    SetObjectiveCompleted(aiObjective, False)
    SetObjectiveFailed(aiObjective, False)
EndFunction

Function CompleteOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function FailOpenObjectives()
    FailOpenObjective(100)
    FailOpenObjective(125)
    FailOpenObjective(200)
    FailOpenObjective(250)
    FailOpenObjective(300)
EndFunction

Function FailOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Function GiveReconWeapon()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || p10mm == None
        Return
    EndIf
    ObjectReference reconRifle = playerRef.PlaceAtMe(p10mm, 1, False, False, False)
    If reconRifle == None
        playerRef.AddItem(p10mm, 1, True)
        Return
    EndIf
    If pmod_10mm_SCOPE_Recon_Base != None
        reconRifle.AttachMod(pmod_10mm_SCOPE_Recon_Base)
    EndIf
    playerRef.AddItem(reconRifle, 1, True)
EndFunction

Function ScheduleEventShutdown(Float afSeconds)
    CancelTimer(31379)
    StartTimer(afSeconds, 31379)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 31379
        Stop()
    EndIf
EndEvent

Function Fragment_Stage_0001_Item_00()
    ResetEventObjectives()
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
    GiveReconWeapon()
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True, True)
    SetObjectiveDisplayed(125, True)
    BoSZ03Script eventScript = EventScript()
    If eventScript != None
        eventScript.BroadcastEventTopic()
        eventScript.StartMarkingPhase()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    BoSZ03Script eventScript = EventScript()
    If eventScript != None
        eventScript.StopMarkingPhase()
    EndIf
    CompleteOpenObjective(100)
    ; Objective 200 carries the 30 s evacuation timer whose expiry sets stage 300.
    SetObjectiveDisplayed(200, True, True)
EndFunction

Function Fragment_Stage_0210_Item_00()
    If IsStageDone(200) || IsStageDone(300)
        Return
    EndIf
    ; Nearly everything is already dead, so the marking requirement gives way to the strike.
    CompleteOpenObjective(100)
    SetStage(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
    CompleteOpenObjective(200)
    SetObjectiveDisplayed(250, True, True)
    BoSZ03Script eventScript = EventScript()
    If eventScript != None
        eventScript.StopMarkingPhase()
        eventScript.BeginArtilleryBarrage()
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    BoSZ03Script eventScript = EventScript()
    If eventScript != None
        eventScript.StopArtilleryBarrage()
    EndIf
    CompleteOpenObjective(250)
    SetObjectiveDisplayed(300, True, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    CompleteOpenObjective(100)
    CompleteOpenObjective(125)
    CompleteOpenObjective(250)
    CompleteOpenObjective(300)
    If !IsStageDone(500)
        SetStage(500)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    BoSZ03Script eventScript = EventScript()
    If eventScript != None
        eventScript.StopMarkingPhase()
        eventScript.StopArtilleryBarrage()
    EndIf
    CompleteOpenObjective(100)
    CompleteOpenObjective(125)
    CompleteOpenObjective(200)
    CompleteOpenObjective(250)
    CompleteOpenObjective(300)
    ScheduleEventShutdown(10.0)
EndFunction

Function Fragment_Stage_8900_Item_00()
    BoSZ03Script eventScript = EventScript()
    If eventScript != None
        eventScript.StopMarkingPhase()
        eventScript.StopArtilleryBarrage()
    EndIf
    FailOpenObjectives()
    ScheduleEventShutdown(10.0)
EndFunction
