Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If DepositMessage == None
        Return
    EndIf

    Float c0 = akActionRef.GetItemCount(DepositOptions[0].ItemForm) as Float
    Float c1 = akActionRef.GetItemCount(DepositOptions[1].ItemForm) as Float
    Int button = DepositMessage.Show(c0, c1)
    If button == 0
        Return
    EndIf

    Int idx = button - 1
    If idx < 0 || idx >= DepositOptions.Length
        Return
    EndIf

    Int amount = akActionRef.GetItemCount(DepositOptions[idx].ItemForm)
    If amount <= 0
        Return
    EndIf

    akActionRef.RemoveItem(DepositOptions[idx].ItemForm, amount)
    akActionRef.ModValue(DepositCountAV, amount as Float * DepositOptions[idx].DepositCountMultiplier)
    If LindaLeeEatSound
        LindaLeeEatSound.Play(self)
    EndIf
    If DepositOptions[idx].ItemMessage
        DepositOptions[idx].ItemMessage.Show()
    EndIf

    If akActionRef.GetValue(DepositCountAV) >= RewardThresholdGlobal.GetValue() && FishingQuest.GetStage() == FishingQuestTurnInStage
        akActionRef.AddItem(RewardList, 1)
        FishingQuest.SetStage(FishingQuestStageToSet)
    EndIf
EndEvent
