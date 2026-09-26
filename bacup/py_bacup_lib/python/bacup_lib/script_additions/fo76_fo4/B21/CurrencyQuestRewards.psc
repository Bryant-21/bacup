Scriptname B21:CurrencyQuestRewards Extends Quest

Int[] Property RewardStages Auto Const
Int[] Property RewardStageItems Auto Const
Form[] Property RewardItems Auto Const
GlobalVariable[] Property RewardAmounts Auto Const
Int[] Property RewardMultipliers Auto Const
Int[] Property HoldingLimits Auto Const

Bool[] GrantedRows

Event OnQuestInit()
    If RewardStages != None
        GrantedRows = New Bool[RewardStages.Length]
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If RewardStages == None || RewardStageItems == None || RewardItems == None || RewardAmounts == None || RewardMultipliers == None || HoldingLimits == None
        Return
    EndIf
    Int index = 0
    While index < RewardStages.Length && index < RewardStageItems.Length && index < RewardItems.Length && index < RewardAmounts.Length && index < RewardMultipliers.Length && index < HoldingLimits.Length
        If RewardStages[index] == auiStageID && RewardStageItems[index] == auiItemID
            GrantRow(index)
        EndIf
        index += 1
    EndWhile
EndEvent

Function GrantRow(Int index)
    If GrantedRows == None
        GrantedRows = New Bool[RewardStages.Length]
    EndIf
    If GrantedRows[index]
        Return
    EndIf
    ; Claim before native calls; quest reinitialization starts a new reward run.
    GrantedRows[index] = True
    Actor playerRef = Game.GetPlayer()
    Form rewardItem = RewardItems[index]
    Int multiplier = RewardMultipliers[index]
    If playerRef == None || rewardItem == None || multiplier <= 0
        Return
    EndIf

    Int amount = 1
    If RewardAmounts[index] != None
        amount = RewardAmounts[index].GetValueInt()
    EndIf
    Int owned = playerRef.GetItemCount(rewardItem)
    Int limit = HoldingLimits[index]
    If amount <= 0 || owned < 0 || limit <= owned
        Return
    EndIf
    Int room = limit - owned
    Int count = room
    If amount <= room / multiplier
        count = amount * multiplier
    EndIf
    If count > 0
        playerRef.AddItem(rewardItem, count)
    EndIf
EndFunction
