; Each of up to three contributions of NumMeatPerActivate Prime Meat sets the personal stage 100/200/300 and
; deepens the Meat Sweats (level 1, 2, 3).
Event OnActivate(ObjectReference akActionRef)
    PlayerRef = Game.GetPlayer()
    OwningQuest = GetOwningQuest()
    If akActionRef != PlayerRef || OwningQuest == None || E02A_Meat_BBQ == None || !E02A_Meat_BBQ.IsRunning()
        Return
    EndIf
    Int contribution = 1
    If OwningQuest.IsStageDone(300)
        If MaxContributionsMessage != None
            MaxContributionsMessage.Show()
        EndIf
        Return
    ElseIf OwningQuest.IsStageDone(200)
        contribution = 3
    ElseIf OwningQuest.IsStageDone(100)
        contribution = 2
    EndIf
    If E02A_Meat_PrimeMeat == None || PlayerRef.GetItemCount(E02A_Meat_PrimeMeat) < NumMeatPerActivate
        If ErrorSound != None
            ErrorSound.Play(PlayerRef)
        EndIf
        If NeedItemMessage != None
            NeedItemMessage.Show()
        EndIf
        Return
    EndIf
    PlayerRef.RemoveItem(E02A_Meat_PrimeMeat, NumMeatPerActivate)
    OwningQuest.SetStage(contribution * 100)
    PlayerRef.RemoveSpell(MeatSweats_Lvl1)
    PlayerRef.RemoveSpell(MeatSweats_Lvl2)
    If contribution == 1
        PlayerRef.AddSpell(MeatSweats_Lvl1, False)
        ShowMessage(MeatSweatsMessage)
    ElseIf contribution == 2
        PlayerRef.AddSpell(MeatSweats_Lvl2, False)
        ShowMessage(MeatSweatsIntesifiedMessage)
    Else
        PlayerRef.AddSpell(MeatSweats_Lvl3, False)
        ShowMessage(MeatSweatsIntesifiedMessage)
    EndIf
EndEvent

Function ShowMessage(Message akMessage)
    If akMessage != None
        akMessage.Show()
    EndIf
EndFunction
