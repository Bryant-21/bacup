; Stage 9100 ends Beckett's CAMP finale conversation (INFOs 5A0F38 friend /
; 5A0F43 romance both set it). Thicker Than Water stage 1000 has no other setter:
; its fragment completes the "Speak with Beckett at C.A.M.P." objective, records
; the finale AV and closes this quest, and this QF is the only script that binds
; COMP_Quest_Outro_Full_Beckett. The romance AV is left to its own carriers
; because both branches share this stage.
Function Fragment_Stage_9100_Item_00()
    If pCOMP_Quest_Outro_Full_Beckett && pCOMP_Quest_Outro_Full_Beckett.IsRunning() && pCOMP_Quest_Outro_Full_Beckett.IsStageDone(900) && !pCOMP_Quest_Outro_Full_Beckett.IsStageDone(1000)
        pCOMP_Quest_Outro_Full_Beckett.SetStage(1000)
    EndIf
EndFunction
