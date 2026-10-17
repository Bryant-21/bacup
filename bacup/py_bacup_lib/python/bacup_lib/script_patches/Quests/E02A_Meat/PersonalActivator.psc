; Contributing ItemToRemove (NumToRemove, or all of it with RequireAll) adds ModPercentage to MasterQuest's
; progress bar, then the activator cools down for CooldownTime.
Function Contribute(ObjectReference akActionRef)
    PlayerRef = Game.GetPlayer()
    If akActionRef != PlayerRef || MasterQuest == None || !MasterQuest.IsRunning() || ItemToRemove == None
        Return
    EndIf
    Int count = PlayerRef.GetItemCount(ItemToRemove)
    If count < NumToRemove || count <= 0
        If ErrorSound != None
            ErrorSound.Play(PlayerRef)
        EndIf
        If NeedItemMessage != None
            NeedItemMessage.Show()
        EndIf
        Return
    EndIf
    Int taken = NumToRemove
    If RequireAll
        taken = count
    EndIf
    PlayerRef.RemoveItem(ItemToRemove, taken)
    ProgressBarQuest = MasterQuest
    ProgressBar = MasterQuest as Quests:_Default:ProgressBar:MasterScript
    If ProgressBar != None
        ProgressBar.ModProgress(ModPercentage)
    EndIf
    If GlobalToIncrement != None
        GlobalToIncrement.Mod(NumToIncrementGlobal)
    EndIf
    If SuccessSound != None
        SuccessSound.Play(PlayerRef)
    EndIf
    If SuccessScene != None && !SuccessScene.IsPlaying()
        SuccessScene.Start()
    EndIf
    If CooldownTime > 0.0
        GoToState("cooldown")
    EndIf
EndFunction

Auto State available
    Event OnActivate(ObjectReference akActionRef)
        Contribute(akActionRef)
    EndEvent
EndState

State cooldown
    Event OnBeginState(String asOldState)
        StartTimer(CooldownTime, 1)
    EndEvent

    Event OnActivate(ObjectReference akActionRef)
        If akActionRef == Game.GetPlayer() && CooldownMessage != None
            CooldownMessage.Show()
        EndIf
    EndEvent

    Event OnTimer(Int aiTimerID)
        GoToState("available")
    EndEvent
EndState
