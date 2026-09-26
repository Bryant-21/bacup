Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || IsActivationBlocked()
        Return
    EndIf
    If DepositMessage == None || DepositOptions == None || DepositOptions.Length < 2 || DepositValues == None || DepositCountAV == None || RewardThresholdGlobal == None || RewardList == None
        Return
    EndIf
    BlockActivation(True)
    Fishing_Deposit(akActionRef)
    BlockActivation(False)
EndEvent

Function Fishing_Deposit(ObjectReference depositor)
    Int option = 0
    Bool tutorial = FishingQuest != None && FishingQuest.IsRunning() && FishingQuest.GetStage() == FishingQuestTurnInStage
    If !tutorial || OverrideDepositMessage == None
        Int selection = DepositMessage.Show(depositor.GetItemCount(DepositOptions[0].ItemForm), depositor.GetItemCount(DepositOptions[1].ItemForm))
        option = selection - 1
    EndIf
    If option < 0 || option >= DepositOptions.Length || DepositOptions[option].ItemForm == None
        Return
    EndIf
    Message amountMenu = DepositOptions[option].ItemMessage
    If tutorial && OverrideDepositMessage != None
        amountMenu = OverrideDepositMessage
    EndIf
    If amountMenu == None
        Return
    EndIf
    Int choice = amountMenu.Show(depositor.GetItemCount(DepositOptions[option].ItemForm))
    If choice <= 0
        Return
    EndIf
    Int amount = 0
    Int index = 0
    While index < DepositValues.Length
        If DepositValues[index].MessageIndex == choice
            amount = DepositValues[index].DepositCount
        EndIf
        index += 1
    EndWhile
    Int available = depositor.GetItemCount(DepositOptions[option].ItemForm)
    If amount == -1
        amount = available
    EndIf
    If amount <= 0 || amount > available || DepositOptions[option].DepositCountMultiplier <= 0.0
        Return
    EndIf

    depositor.RemoveItem(DepositOptions[option].ItemForm, amount)
    Float balance = depositor.GetValue(DepositCountAV) + amount * DepositOptions[option].DepositCountMultiplier
    Float threshold = RewardThresholdGlobal.GetValue()
    If tutorial && FishingQuest.GetStage() == FishingQuestTurnInStage && balance >= 3.0
        depositor.AddItem(RewardList, 1)
        balance -= 3.0
        FishingQuest.SetStage(FishingQuestStageToSet)
    EndIf
    If threshold > 0.0
        While balance >= threshold
            depositor.AddItem(RewardList, 1)
            balance -= threshold
        EndWhile
    EndIf
    depositor.SetValue(DepositCountAV, balance)
    If LindaLeeEatSound != None
        LindaLeeEatSound.Play(Self)
    EndIf
EndFunction
