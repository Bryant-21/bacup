Function ResetDeposits()
    OwningQuest = GetOwningQuest()
    VenomCount = 0
    CurrentThreshold = 0
    B21DepositBusy = False
    PublishVenomCount()
EndFunction

Function PublishVenomCount()
    If OwningQuest == None
        Return
    EndIf
    B21:QuestVariables questVariables = OwningQuest as B21:QuestVariables
    If questVariables != None && CurrentCountTextVar != ""
        questVariables.SetVariable(CurrentCountTextVar, VenomCount as Float)
    EndIf
EndFunction

Int Function CurrentRequirement()
    Int requirement = InitialRequirement
    Int index = 0
    While StageThresholds != None && index < CurrentThreshold && index < StageThresholds.Length
        If StageThresholds[index].NewRequirement >= 0
            requirement = StageThresholds[index].NewRequirement
        EndIf
        index += 1
    EndWhile
    Return requirement
EndFunction

Function ApplyThresholds()
    While StageThresholds != None && CurrentThreshold < StageThresholds.Length && VenomCount >= StageThresholds[CurrentThreshold].DepositThreshold
        Int stage = StageThresholds[CurrentThreshold].StageToSet
        CurrentThreshold += 1
        If stage >= 0 && !OwningQuest.IsStageDone(stage)
            OwningQuest.SetStage(stage)
        EndIf
    EndWhile
EndFunction

Bool Function DepositWindowOpen()
    If OwningQuest == None || !OwningQuest.IsRunning()
        Return False
    EndIf
    ; The tub accepts venom once the truck blows and the stills are under attack, until the jamboree resolves.
    Return OwningQuest.IsStageDone(200) && !OwningQuest.IsStageDone(1000) && !OwningQuest.IsStageDone(2000) && !OwningQuest.IsStageDone(3000)
EndFunction

Event OnAliasInit()
    ResetDeposits()
EndEvent

Event OnAliasShutdown()
    B21DepositBusy = False
EndEvent

Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akActionRef == None || akActionRef != playerRef || E08A_GulperVenom == None
        Return
    EndIf
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
    EndIf
    If B21DepositBusy || !DepositWindowOpen()
        Return
    EndIf
    B21DepositBusy = True

    Int needed = CurrentRequirement() - VenomCount
    Int carried = playerRef.GetItemCount(E08A_GulperVenom)
    While needed > 0 && carried > 0 && DepositWindowOpen()
        Int amount = carried
        If amount > needed
            amount = needed
        EndIf
        playerRef.RemoveItem(E08A_GulperVenom, amount, False, None)
        Int remaining = playerRef.GetItemCount(E08A_GulperVenom)
        Int deposited = carried - remaining
        If deposited > amount
            deposited = amount
        EndIf
        If deposited <= 0
            needed = 0
        Else
            VenomCount += deposited
            PublishVenomCount()
            ApplyThresholds()
            ; Reaching a threshold can raise the requirement, so the rest of the carried venom may still fit.
            needed = CurrentRequirement() - VenomCount
        EndIf
        carried = remaining
    EndWhile

    B21DepositBusy = False
EndEvent
