use anchor_lang::prelude::*;
use anchor_lang::solana_program::native_token::{lamports_to_sol, sol_to_lamports};
use anchor_lang::system_program;

use std::mem::size_of;

declare_id!("34T7bkuZbvy5YDKxEk1YUuNbugrb75LdpWLCyCzbgtyz");

pub mod constants {
    pub const MAX_ENDING_TIME: u64 = 2_592_000; // 30 days
}

pub mod helpers {
    //
}

#[program]
pub mod crowdfund {
    use super::*;

    pub fn create_crowd_fund(
        ctx: Context<CreateCrowdFund>,
        title: String,
        goal: f64,
        starts_at: u32,
        ends_at: u32,
        treasury: Pubkey
    ) -> Result<()> {
        let crowd_fund: &mut Account<CrowdFund> = &mut ctx.accounts.crowd_fund;
        let current_time = Clock::get().unwrap().unix_timestamp as u64;

        require!(
            starts_at > current_time,
            Errors::InvalidStartingTime
        );
        require!(
            ends_at > starts_at,
            Errors::InvalidEndingTime
        );
        require!(
            ends_at <= current_time + constants::MAX_ENDING_TIME,
            Errors::ExceedEndingTime
        );
        require!(
            goal >= 1.0,
            Errors::InvalidGoalAmount
        );
        require!(
            title.len() >= 10,
            Errors::InvalidTitleLength
        );

        crowd_fund.bump = *ctx.bumps.get("crowd_fund").unwrap();
        crowd_fund.owner = ctx.accounts.owner.key();
        crowd_fund.goal = sol_to_lamports(goal);
        crowd_fund.starts_at = starts_at;
        crowd_fund.ends_at = ends_at;
        crowd_fund.treasury = treasury;

        msg!("New crowd fund created - {:?}", crowd_fund.key());

        emit!(NewCrowdFundCreated {
            crowd_fund: crowd_fund.key()
        });
        
        Ok(()) 
    }

    pub fn create_pledge_account(
        ctx: Context<CreatePledge>,
        crowd_fund_title: String,
        amount_to_pledge: f64
    ) -> Result<()> {
        let current_time = Clock::get().unwrap().unix_timestamp as u64;
        let crowd_fund: &Account<CrowdFund> = &ctx.accounts.crowd_fund;
        let user: &mut Account<User> = &mut ctx.accounts.pledge;

        require!(
            crowd_fund.starts_at < current_time,
            Errors::CrowdFundNotStarted
        );
        require!(
            crowd_fund.ends_at > current_time,
            Errors::CrowdFundEnded
        );

        user.crowd_fund = crowd_fund.key();
        user.pledger = ctx.accounts.user.key();
        user.bump = *ctx.bumps.get("pledge").unwrap();

        msg!("New user's pledge account created - {:?}", user.key());

        emit!(NewPledgeAccountCreated {
            crowd_fund: crowd_fund.key(),
            user_pledge_account: user.key()
        });

        Ok(())
    }

    pub fn pledge(
        ctx: Context<Pledge>,
        crowd_fund_title: String,
        amount_to_pledge: f64
    ) -> Result<()> {
        let user_pledge_account: &mut Account<User> = &mut ctx.accounts.pledge;
        let crowd_fund: &Account<CrowdFund> = &ctx.accounts.crowd_fund;
        let current_time = Clock::get().unwrap().unix_timestamp as u64;
        let lamports = sol_to_lamports(amount_to_pledge);

        require!(
            lamports > 0,
            Errors::ZeroPledge
        );
        require!(
            crowd_fund.starts_at < current_time,
            Errors::CrowdFundNotStarted
        );
        require!(
            crowd_fund.ends_at > current_time,
            Errors::CrowdFundEnded
        );

        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.user.to_account_info().clone(),
                to: user_pledge_account.to_account_info().clone()
            }
        );
        system_program::transfer(cpi_context, lamports).unwrap();

        user_pledge_account.pledged_amount.checked_add(lamports).unwrap();

        msg!("Pledged successfully - {:?} SOL.", amount_to_pledge);

        emit!(PledgedTo {
            crowd_fund: crowd_fund.key(),
            user_pledge_account: user_pledge_account.key(),
            pledge_amount: amount_to_pledge
        });

        Ok(())
    }

    pub fn unpledge(
        ctx: Context<Unpledge>,
        crowd_fund_title: String,
        amount_to_unpledge: f64
    ) -> Result<()> {
        let crowd_fund: &Account<CrowdFund> = &ctx.accounts.crowd_fund;
        let user_pledge_account: &mut Account<User> = &mut ctx.accounts.pledge;
        let current_time = Clock::get().unwrap().unix_timestamp as u64;
        let lamports = sol_to_lamports(amount_to_unpledge);
        let user_balance = user_pledge_account.pledged_amount;

        require!(
            crowd_fund.starts_at < current_time,
            Errors::CrowdFundNotStarted
        );
        require!(
            crowd_fund.ends_at > current_time,
            Errors::CrowdFundEnded
        );
        require!(
            lamports <= user_balance,
            Errors::NotEnoughBalance
        );

        **user_pledge_account.to_account_info().try_borrow_mut_lamports()? -= lamports;
        **ctx.accounts.user.to_account_info().try_borrow_mut_lamports()? += lamports;

        user_pledge_account.pledged_amount.checked_sub(lamports).unwrap();

        msg!("Unpledged from the crowd_fund - {:?} SOL.", lamports_to_sol(lamports));

        emit!(UnpledgedFrom {
            crowd_fund: crowd_fund.key(),
            user_pledge_account: user_pledge_account.key(),
            unpledge_amount: amount_to_unpledge
        });

        Ok(())
    }

    pub fn claim(
        ctx: Context<Claim>,
        crowd_fund_title: String,
        pledger: Pubkey,
        user: Pubkey
    ) -> Result<()> {
        let crowd_fund: &Account<CrowdFund> = &ctx.accounts.crowd_fund;
        let user_pledge_account: &mut Account<User> = &mut ctx.accounts.pledge;
        let treasury = &mut ctx.accounts.treasury;
        
        user_pledge_account.is_claimed = true;

        **user_pledge_account.to_account_info().try_borrow_mut_lamports()? -= user_pledge_account.pledged_amount;
        **treasury.to_account_info().try_borrow_mut_lamports()? += user_pledge_account.pledged_amount;

        msg!("Successfully claimed - {:?}", lamports_to_sol(user_pledge_account.pledged_amount));

        emit!(Claimed {
            crowd_fund: crowd_fund.key(),
            user_pledge_account: user_pledge_account.key()
        });
        
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(title: String)]
pub struct CreateCrowdFund<'info> {
    #[account(
        init,
        payer = owner,
        space = 8 + 32 + 32 + 8 + 8 + 8 + 1 + (4 + &title.len()),
        seeds = [ b"crowd_fund".as_ref(), &title.as_bytes().as_ref() ],
        bump
    )]
    pub crowd_fund: Account<'info, CrowdFund>,
    #[account(mut)]
    pub owner: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(crowd_fund_title: String)]
pub struct CreatePledge<'info> {
    #[account(
        seeds = [ b"crowd_fund".as_ref(), &crowd_fund_title.as_bytes().as_ref() ],
        bump = crowd_fund.bump
    )]
    pub crowd_fund: Account<'info, CrowdFund>,
    #[account(
        init,
        payer = user,
        space = 8 + size_of::<User>(),
        seeds = [ b"pledge".as_ref(), crowd_fund.key().as_ref(), user.key().as_ref() ],
        bump
    )]
    pub pledge: Account<'info, User>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(crowd_fund_title: String)]
pub struct Pledge<'info> {
    #[account(
        seeds = [ b"crowd_fund".as_ref(), &crowd_fund_title.as_bytes().as_ref() ],
        bump = crowd_fund.bump
    )]
    pub crowd_fund: Account<'info, CrowdFund>,
    #[account(
        mut,
        seeds = [ b"pledge".as_ref(), crowd_fund.key().as_ref(), user.key().as_ref() ],
        bump = pledge.bump
    )]
    pub pledge: Account<'info, User>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
#[instruction(crowd_fund_title: String)]
pub struct Unpledge<'info> {
    #[account(
        seeds = [ b"crowd_fund".as_ref(), &crowd_fund_title.as_bytes().as_ref() ],
        bump = crowd_fund.bump
    )]
    pub crowd_fund: Account<'info, CrowdFund>,
    #[account(
        mut,
        seeds = [ b"pledge".as_ref(), crowd_fund.key().as_ref(), user.key().as_ref() ],
        bump = pledge.bump
    )]
    pub pledge: Account<'info, User>,
    #[account(mut)]
    pub user: Signer<'info>
}

#[derive(Accounts)]
#[instruction(crowd_fund_title: String, pledger: Pubkey)]
pub struct Claim<'info> {
    #[account(
        seeds = [ b"crowd_fund".as_ref(), &crowd_fund_title.as_bytes().as_ref() ],
        bump = crowd_fund.bump,
        has_one = owner @ Errors::OnlyOwner,
        has_one = treasury @ Errors::InvalidTreasury,
        constraint = (crowd_fund.ends_at < (clock.unix_timestamp as u32)) @ Errors::CrowdFundNotEnded
    )]
    pub crowd_fund: Account<'info, CrowdFund>,
    /// CHECK: It is safe. We are just sending lamports to this account we are not going to read any data from this account or ... .
    #[account(mut)]
    pub treasury: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [ b"pledge".as_ref(), crowd_fund.key().as_ref(), &pledger.as_ref() ],
        bump = pledge.bump,
        constraint = pledge.is_claimed == false @ Errors::CannotClaimTwice,
        constraint = pledge.pledged_amount > 0 @ Errors::ZeroPledgedAmount
    )]
    pub pledge: Account<'info, User>,
    #[account(mut)]
    pub owner: Signer<'info>,
    pub clock: Sysvar<'info, Clock>
}

#[account]
#[derive(Default)]
pub struct CrowdFund {
    owner: Pubkey,
    treasury: Pubkey, // After ending a crowd fund, owner must call claim instruction and funds will transfered to this account
    goal: u64,
    starts_at: u64,
    ends_at: u64,
    bump: u8,
    title: String
}

#[account]
#[derive(Default)]
pub struct User {
    crowd_fund: Pubkey,
    pledger: Pubkey,
    pledged_amount: u64,
    is_claimed: bool,
    bump: u8
}

#[event]
pub struct NewCrowdFundCreated {
    crowd_fund: Pubkey
}

#[event]
pub struct NewPledgeAccountCreated {
    crowd_fund: Pubkey,
    user_pledge_account: Pubkey
}

#[event]
pub struct PledgedTo {
    crowd_fund: Pubkey,
    user_pledge_account: Pubkey,
    pledge_amount: f64
}

#[event]
pub struct UnpledgedFrom {
    crowd_fund: Pubkey,
    user_pledge_account: Pubkey,
    unpledge_amount: f64
}

#[event]
pub struct Claimed {
    crowd_fund: Pubkey,
    user_pledge_account: Pubkey
}

#[error_code]
pub enum Errors {
    #[msg("Starting time < current time.")]
    InvalidStartingTime,
    #[msg("Ending time < Starting time.")]
    InvalidEndingTime,
    #[msg("Max ending timestamp <= 30 days.")]
    ExceedEndingTime,
    #[msg("Goal must be >= 1 SOL.")]
    InvalidGoalAmount,
    #[msg("Title length >= 10.")]
    InvalidTitleLength,
    #[msg("Crowd fund has been ended.")]
    CrowdFundEnded,
    #[msg("Crowd fund not started yet.")]
    CrowdFundNotStarted,
    #[msg("Cannot pledge zero amount.")]
    ZeroPledge,
    #[msg("Not enough balance.")]
    NotEnoughBalance,
    #[msg("Crowd fund not ended yet.")]
    CrowdFundNotEnded,
    #[msg("Only owner of the crowd fund can claim.")]
    OnlyOwner,
    #[msg("Zero pledge amount.")]
    ZeroPledgedAmount,
    #[msg("Already claimed.")]
    CannotClaimTwice,
    #[msg("Invalid treasury account.")]
    InvalidTreasury
}
